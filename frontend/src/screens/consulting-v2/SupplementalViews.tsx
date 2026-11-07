import { useState } from 'react';
import { DASH, num, nz, pct1, yen } from './format';
import { Async, CaseLink, Fresh, HubspotLink } from './parts';
import { Bars, BoxPlot, DataTable } from './ResultCharts';
import { DealTable, TODAY_COLUMNS } from './DealTable';
import type { Route } from './route';
import type { BoardResponse, FocusResponse, StageStayResponse } from './types';
import { useApi } from './useApi';

type Go = (r: Partial<Route>) => void;
export function StageStay({ consultant, go }: { consultant: string; go: Go }) {
  const [reload, setReload] = useState(0);
  const result = useApi<StageStayResponse>('/api/consulting/stage-stay', reload);
  return <details className="cv2-more"><summary>滞留した案件</summary><Async result={result} onRetry={() => { setReload((n) => n + 1); }}>{(d) => {
    const rows = d.rows.filter((r) => consultant === '' || r.consultant === consultant);
    const table = (title: string, escalation: boolean) => <><h3>{title}</h3><DataTable title={title} heads={['案件', '担当', 'ステージ', '入った日', '滞留日数', '満了日', '満了後（日）', 'リンク']} rows={rows.filter((r) => escalation ? r.escalation : !r.escalation && r.stay).map((r) => [<button type="button" className="cv2-link" onClick={() => { go({ view: 'detail', id: r.deal_id }); }}>{r.name ?? '取引名なし'}</button>, r.consultant ?? DASH, r.stage_label ?? DASH, r.entered ?? '履歴なし', nz(r.days), r.expiry ?? DASH, nz(r.days_past_expiry), <><CaseLink dealId={r.deal_id} /> <HubspotLink portal={d.meta.hubspot_portal_id} dealId={r.deal_id} /></>])} /></>;
    return <>
      <Fresh meta={d.meta} /><p>同じステージに {num(d.meta.stay_days)} 日を超えて止まっている案件と、満了後も稼働中の案件です。ステージは HubSpot で直します。</p>
      <p className="cv2-dim">全社の稼働中 {num(d.meta.n_active)} 件。滞留 {num(d.meta.n_stay)} 件・満了後も稼働中 {num(d.meta.n_escalation)} 件（重なります）。入った日が分からない案件 {num(d.meta.n_unknown_entered)} 件は滞留に数えていません。{consultant !== '' ? `表は ${consultant} で絞っています。` : ''}</p>
      {table('満了後も稼働中の案件', true)}{table('同じステージに長く止まっている案件', false)}
    </>;
  }}</Async></details>;
}
export function Overdue({ go }: { go: Go }) {
  const [reload, setReload] = useState(0);
  const result = useApi<BoardResponse>('/api/consulting/deals', reload);
  return <details className="cv2-more"><summary>満了日超過の稼働中（今月の満了も含む）</summary><Async result={result} onRetry={() => { setReload((n) => n + 1); }}>{(d) => {
    const rows = d.rows.filter((r) => r.overdue_active === true).sort((a, b) => (a.days_left ?? 0) - (b.days_left ?? 0));
    return <><Fresh meta={d.meta} /><p>満了日を過ぎてもステージが稼働中の契約 {num(rows.length)} 件です。過ぎた日数の多い順。HubSpot でステージを確認してください。</p>
      <DealTable caption="満了日超過の稼働中" rows={rows} columns={TODAY_COLUMNS} portal={d.meta.hubspot_portal_id} onOpen={(id) => { go({ view: 'detail', id }); }} />
    </>;
  }}</Async></details>;
}
export function Focus({ go }: { go: Go }) {
  const [reload, setReload] = useState(0);
  const result = useApi<FocusResponse>('/api/consulting/focus', reload);
  return <details className="cv2-more"><summary>注目顧客（満足度・採用単価・MTG の記録）</summary><Async result={result} onRetry={() => { setReload((n) => n + 1); }}>{(d) => {
    const n = d.nps_low, c = d.cpa, m = d.mtg_layers;
    return <><Fresh meta={d.meta} />
      <h3>満足度が {n.threshold} 以下の顧客（{num(n.n)}件）</h3>
      <p>回答あり {num(n.n_have_nps)} / 稼働中 {num(n.n_act)} 件（{pct1(n.coverage)}）。{n.note}</p>
      <Bars title="満足度の分布" rows={n.dist.map((r) => ({ label: `${num(r.v)}点`, n: r.n }))} />
      <DataTable title="満足度が低い顧客" heads={['案件', '満足度', '回答月', '金額', '満了まで（日）', '接触回数', 'リンク']} rows={n.rows.map((r) => [<button type="button" className="cv2-link" onClick={() => { go({ view: 'detail', id: r.deal_id }); }}>{r.name ?? '取引名なし'}</button>, r.nps, r.nps_month, yen(r.amount), nz(r.days_to_expiry), r.n_contact, <><CaseLink dealId={r.deal_id} /> <HubspotLink portal={d.meta.hubspot_portal_id} dealId={r.deal_id} /></>])} />
      <h3>採用単価が悪化した拠点（{num(c.worse)} / {num(c.judged)} 拠点）</h3><p className="cv2-dim">{c.note}。未確定のため見送り {num(c.skipped_censored)} 拠点・拠点が空 {num(c.skipped_no_site)} 取引。</p>
      <DataTable title="採用単価が悪化した拠点" heads={['拠点', '前回', '直近', '倍率', '確定した点の数']} rows={c.rows.map((r) => [r.site_name ?? '拠点名なし', yen(r.prev), yen(r.last), r.ratio === null ? DASH : num(r.ratio, 2), r.n_points])} />{c.truncated === true && <p className="cv2-dim">倍率の高い順に60拠点まで表示しています。</p>}
      <h3>MTG の記録（稼働中 {num(m.n_act)}件）</h3><Bars title="MTG の記録の内訳" rows={[{ label: '録画とメール両方', n: m.both }, { label: '録画だけ（事実）', n: m.only_recording }, { label: 'メールだけ（推定）', n: m.only_mail }, { label: 'どちらも記録なし', n: m.neither }]} /><p className="cv2-dim">{m.note}。記録が見つからないことと、実施していないことは別です。</p>
      <h3>累計契約金額の散らばり（{num(d.shape.n_display)}社）</h3><BoxPlot title="累計契約金額の散らばり（円）" rows={[{ label: '稼働中の取引を持つ法人', stats: d.shape.ltv }]} /><p className="cv2-dim">複数拠点 {num(d.shape.multi_site)} 社。{d.shape.multi_site_note}</p>
    </>;
  }}</Async></details>;
}
