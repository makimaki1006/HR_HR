import { useState } from 'react';
import { DASH, num, nz, pct1, yen } from './format';
import { Async, NoData, Section } from './parts';
import { Bars, BoxPlot, CHART_MIN_N, DataTable } from './ResultCharts';
import type { ResultsResponse, DataQualityResponse, Rampup } from './types';
import { useApi } from './useApi';
import type { Route } from './route';

const labels: Record<string, string> = { oubo: '応募数', mensetu: '面接数', syoudaku: '承諾数', saiyomokuhyou: '採用目標', keisaisu: '掲載数', deal: '取引由来', company: '会社由来', none: '無し' };
function Launch({ launch: l }: { launch: NonNullable<Rampup['launch']> }) {
  const maxM = Math.max(1, ...l.cohorts.flatMap((c) => c.points.filter((p) => p.n >= CHART_MIN_N && p.mean !== null).map((p) => p.m)));
  const hi = Math.max(1, ...l.cohorts.flatMap((c) => c.points.filter((p) => p.n >= CHART_MIN_N).map((p) => p.mean ?? 0)));
  const colors = ['#225d96', '#0f6b51', '#8b541b', '#5a3e8f', '#4a6572', '#2d8a8a'];
  const x = (m: number): number => 55 + (m - 1) / Math.max(1, maxM - 1) * 610;
  const y = (v: number): number => 210 - v / hi * 180;
  return <Section title="初回契約の開始月ごとの応募の推移">
    <p>初回契約（売上回数が1回）だけです。対象 {num(l.n_first)} 件。記録なし {num(l.n_no_series)} 件・累計が減った {num(l.n_decrease)} 取引の月は除いています。{(l.n_sales_unread ?? 0) > 0 && `売上回数を読めなかった ${num(l.n_sales_unread ?? 0)} 件は含めていません。`}</p>
    <svg className="cv2-chart" viewBox="0 0 720 250" role="img" aria-label="初回契約の開始月ごとの応募の推移">
      {[0, hi].map((v) => <g key={v}><line x1="55" x2="665" y1={y(v)} y2={y(v)} stroke="#dae2eb" /><text x="45" y={y(v)} textAnchor="end" fontSize="11">{num(v, 1)}</text></g>)}
      {Array.from({ length: maxM }, (_, i) => <text key={i} x={x(i + 1)} y="235" textAnchor="middle" fontSize="11">{i + 1}ヶ月目</text>)}
      {l.cohorts.map((c, i) => {
        let connected = false;
        const path = c.points.map((p) => {
          if (p.n < CHART_MIN_N || p.mean === null) { connected = false; return ''; }
          const part = `${connected ? 'L' : 'M'}${String(x(p.m))},${String(y(p.mean))}`; connected = true; return part;
        }).join(' ');
        return <g key={c.start}><path d={path} fill="none" stroke={colors[i % colors.length]} strokeWidth="2" />
          {c.points.filter((p) => p.n >= CHART_MIN_N && p.mean !== null).map((p) => <circle key={p.m} cx={x(p.m)} cy={y(p.mean ?? 0)} r="4" fill={colors[i % colors.length]}><title>{c.start} 開始 {p.m}ヶ月目: {p.mean}（{p.n}件）</title></circle>)}
        </g>;
      })}
    </svg>
    <ul className="cv2-legend">{l.cohorts.map((c, i) => <li key={c.start}><i style={{ background: colors[i % colors.length] }} />{c.start} 開始（{num(c.n_deals)}件）</li>)}</ul>
    <p className="cv2-dim">30件未満の点は打っていません。1取引あたりの月別の応募数です。まだ終わっていない月、満了月より後の月は線を伸ばしていません。開始時期と応募の増え方の関係は相関で、原因とは限りません。</p>
    <details className="cv2-more"><summary>数字を表で見る（図に載せない月も含む）</summary><DataTable title="開始月ごとの応募数" heads={['開始月', '何ヶ月目', '取引数', '平均応募数', '図への表示']} rows={l.cohorts.flatMap((c) => c.points.map((p) => [c.start, p.m, p.n, p.mean === null ? DASH : num(p.mean, 1), p.mean === null ? '記録なし' : p.n < CHART_MIN_N ? '30件未満のため非表示' : '表示']))} /></details>
  </Section>;
}
export function ResultsDetails({ d, go }: { d: ResultsResponse; go: (r: Partial<Route>) => void }) {
  const by = d.renewal?.by_renewal ?? [];
  const ramp = d.rampup;
  const risk = d.outcome?.risk;
  const cs = d.outcome?.contact_source;
  return <>
    <Section title="継続回数ごとの成果のばらつき">
      {(['oubo', 'syoudaku', 'oubo_per_posting'] as const).map((key) => {
        const title = `${key === 'oubo_per_posting' ? '求人票あたり応募' : labels[key] ?? '成果'}の散らばり（継続回数ごと）`;
        return <div key={key}><h3>{title}</h3><BoxPlot title={title} rows={by.map((r) => ({ label: `継続${num(r.renewal_no)}回`, stats: r[key], eligible: r.denom >= CHART_MIN_N }))} /></div>;
      })}
      <p className="cv2-dim">{d.renewal?.meta?.exclude_right_censored === true ? '結果がまだ出ていない最近の契約を外しています。' : `結果がまだ出ていない最近の契約を含んでいます${d.renewal?.meta?.right_censored_n === undefined ? '' : `（${num(d.renewal.meta.right_censored_n)}件）`}。直近の契約は成果が低く出ることがあります。`}</p>
      <details className="cv2-more"><summary>継続回数ごとの代表値</summary><DataTable title="継続回数ごとの代表値" heads={['継続回数', '応募数', '面接数', '承諾数', '求人票あたり応募', '契約額']} rows={by.map((r) => [`継続${num(r.renewal_no)}回`, nz(r.oubo?.median), nz(r.mensetu?.median), nz(r.syoudaku?.median), r.oubo_per_posting ? num(r.oubo_per_posting.median, 1) : DASH, yen(r.amount?.median)])} /><p className="cv2-dim">各項目の中央値です。</p></details>
    </Section>
    <Section title="立ち上がり">
      {ramp ? <>
        <h3>契約の段階</h3><p className="cv2-dim">{ramp.phase.rule}。期間が入っていないものは「出せない」に置きます。</p><Bars title="契約の段階" rows={ramp.phase.rows} />
        <h3>初回MTGまでの日数の散らばり</h3><p>契約前のMTG {num(ramp.first_mtg.pre_contract)} 件は含めていません。</p>
        <BoxPlot title="初回MTGまでの日数" rows={[{ label: '初回MTGまで', stats: ramp.first_mtg.stats }]} />
        <h3>区分ごとの、その後の解約率</h3><Bars title="初回MTGまでの区分別解約率" rows={ramp.first_mtg.buckets.filter((b) => b.denom >= CHART_MIN_N).map((b) => ({ label: b.label, n: b.cancel_rate }))} percent />
        <DataTable title="初回MTGまでの区分別解約率" heads={['区分', '契約数', '結果が出た件数', '解約・充足', '解約率']} rows={ramp.first_mtg.buckets.map((b) => [b.label, b.n, b.denom, b.cancel, pct1(b.cancel_rate)])} />
        <p className="cv2-dim">{ramp.first_mtg.note}。結果が出た件数が30件未満の区分は図に載せていません。</p>
      </> : <NoData label="立ち上がり" />}
    </Section>
    {ramp?.launch && <Launch launch={ramp.launch} />}
    <Section title="リスクの2軸と最優先">
      {risk ? <>
        <p>稼働中 {num(risk.n_act)} 件。2軸とも赤が最優先です。</p><Bars title="リスクの区分" rows={risk.bands} />
        <DataTable title="リスクの2軸" heads={['軸', '赤', '白', '未測定', '基準']} rows={[[ '放置', risk.ax3['赤'], risk.ax3['白'], risk.ax3['未測定'], risk.ax3.rule ], ['収益', risk.ax4['赤'], risk.ax4['白'], risk.ax4['未測定'], risk.ax4.rule]]} />
        <p className="cv2-dim">接触の記録が無い {num(risk.ax3['未測定'])} 件は未測定に分けています。記録が無いことと放置は別です。</p>
        {cs && <p className="cv2-dim">数えた接触: 60秒超の通話 {num(cs.calls_over_threshold)} / MTG {num(cs.mtgs_linked)} / 接触のある取引 {num(cs.deals_with_contact)}</p>}
        <h3>最優先の広がり（金額 × 満了まで）</h3>
        <svg className="cv2-chart" viewBox="0 0 720 280" role="img" aria-label="最優先の広がり">
          <line x1="70" x2="660" y1="230" y2="230" stroke="#607388" /><line x1="70" x2="70" y1="20" y2="230" stroke="#607388" />
          <text x="350" y="270" fontSize="12">満了まで（日）</text><text x="5" y="15" fontSize="12">契約額</text>
          {[0, 60].map((v) => <text key={v} x={70 + v / 60 * 590} y="248" fontSize="11">{v}</text>)}
          <text x="10" y="230" fontSize="11">0</text><text x="5" y="35" fontSize="11">{yen(Math.max(1, ...risk.top.map((r) => r.amount ?? 0)))}</text>
          {risk.top.filter((r) => r.amount !== null && r.days_to_expiry !== null).map((r) => <circle key={r.deal_id} cx={70 + (r.days_to_expiry ?? 0) / 60 * 590} cy={230 - (r.amount ?? 0) / Math.max(1, ...risk.top.map((a) => a.amount ?? 0)) * 195} r="6" stroke={r.no_start || r.never_after_start ? '#ab2f26' : '#8b541b'} fill={r.no_start && !r.never_after_start ? 'white' : r.never_after_start ? '#ab2f26' : '#8b541b'}><title>{r.name}: {yen(r.amount)}、満了まで{r.days_to_expiry}日。{r.ax3w}</title></circle>)}
        </svg>
        <p className="cv2-dim">赤: 契約後に一度も接触なし。茶: 接触から30日超。赤の中空: 開始日が空で、契約後を切り出せない（赤に数えています）。</p>
        <details className="cv2-more"><summary>最優先の案件（{num(risk.top.length)}件）</summary><p>{risk.order_note}</p><DataTable title="2軸とも赤の案件" heads={['案件', '金額', '満了まで（日）', '接触の状況']} rows={risk.top.map((r) => [<button className="cv2-link" type="button" onClick={() => { go({ view: 'detail', id: r.deal_id }); }}>{r.name ?? '取引名なし'}</button>, yen(r.amount), nz(r.days_to_expiry), r.ax3w ?? DASH])} /></details>
      </> : <NoData label="リスクの2軸" />}
    </Section>
  </>;
}
export function DataQuality() {
  const [reload, setReload] = useState(0);
  const result = useApi<DataQualityResponse>('/api/consulting/data-quality', reload);
  return <Section title="データ品質"><Async result={result} onRetry={() => { setReload((n) => n + 1); }}>{(d) => <>
    <p>取引 {num(d.meta.n_deals)} 件（稼働中 {num(d.meta.n_active)} 件）。</p>
    <details className="cv2-more"><summary>データの欠けと記入率の詳細</summary>
      <h3>法人番号の出どころ</h3><DataTable title="法人番号の出どころ" heads={['出どころ', '件数']} rows={d.houjin_source.rows.map((r) => [labels[r.label] ?? r.label, r.n])} /><p className="cv2-dim">{d.houjin_source.note}</p>
      <h3>結果ごとの記入率</h3><p className="cv2-note">{d.outcome_bias.note}</p><DataTable title="結果ごとの記入率" heads={['項目', '結果', '対象', '記入済み', '記入率']} rows={d.outcome_bias.rows.map((r) => [labels[r.field] ?? 'その他の項目', r.group, r.n, r.filled, pct1(r.fill_rate)])} />
      <h3>項目ごとの未入力件数</h3><DataTable title="項目ごとの未入力件数" heads={['項目', '件数', '割合', '読み方']} rows={d.missing.map((r) => [r.label, r.n, pct1(r.rate), r.note])} />
      <h3>読んでいるシート</h3><p className="cv2-dim">件数はいま読めた行数です。行数が分からないものは「—」で示します。</p><DataTable title="読んでいるシート" heads={['シート', '行数']} rows={d.sheets.map((r) => [r.name, nz(r.rows)])} />
    </details>
  </>}</Async></Section>;
}
