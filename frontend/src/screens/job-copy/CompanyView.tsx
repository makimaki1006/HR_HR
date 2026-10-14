import { useEffect, useRef, useState } from 'react';
import { AuthRequiredError } from '../../api/client';
import { CompanySummary } from './CompanySummary';
import { loadVersions } from './listingDetailCache';
import { listingRecord } from './hubspotListings';
import type { HubSpotListing } from './hubspotListings';
import type { JobCopyRecord } from './data';
import type { Billing } from './DealSummary';
import { HUBSPOT_BUSY_MESSAGE, isHubSpotBusy } from './SnapshotErrorNotice';
import { contractLabel, dealName, listingsLabel, loadCompanyDeals } from './companyDeals';
import type { CompanyDeals } from './companyDeals';
import { yen } from './homeModel';
import './home.css';

const failure = (error: Error) => error instanceof AuthRequiredError ? 'Googleでログインしてから再取得してください。' : isHubSpotBusy(error) ? HUBSPOT_BUSY_MESSAGE : '読み込めませんでした。時間を置いて再取得してください。';

/** 顧客（事業所）の画面。案件の一覧と、求人の合計。 */
export function CompanyView({ company, billingByJob, onName, onOpenDeal, onOpenListing }: {
  company: { id: string; name: string };
  billingByJob: Billing;
  /** 名前が分かったら知らせる（URL から直接開いたとき、パンくずの名前になる）。 */
  onName: (name: string) => void;
  onOpenDeal: (deal: { id: string; name: string }) => void;
  onOpenListing: (job: JobCopyRecord) => void;
}) {
  const [deals, setDeals] = useState<CompanyDeals | null>(null);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const action = useRef<AbortController | null>(null);
  const report = useRef(onName);
  useEffect(() => { report.current = onName; });
  useEffect(() => {
    const controller = new AbortController();
    void loadCompanyDeals(company.id).then(result => {
      if (controller.signal.aborted) return;
      if (result.ok) { setDeals(result.data); setError(''); if (result.data.company.name) report.current(result.data.company.name); } else setError(failure(result.error));
    });
    return () => { controller.abort(); };
  }, [company.id]);
  useEffect(() => () => { action.current?.abort(); }, []);
  const open = async (row: HubSpotListing) => {
    action.current?.abort(); const controller = new AbortController(); action.current = controller; setBusy(true); setError('');
    const result = await loadVersions(row.id, { signal: controller.signal });
    if (controller.signal.aborted) return;
    if (result.ok) onOpenListing(listingRecord(result.data, row)); else setError(failure(result.error));
    setBusy(false);
  };
  return <main className="jc-detail jh-home" id="job-details" tabIndex={-1}>
    <header className="jh-head"><div><h1>{company.name || '顧客'}</h1><p>この顧客の案件と求人です。案件を開くと、その案件の求人を比べられます。</p></div></header>
    {error && <p role="alert">{error}</p>}{busy && <p role="status">選んだ求人を読み込んでいます…</p>}
    <section aria-labelledby="jh-deals"><h2 id="jh-deals">案件</h2>
      <p className="jh-note">納品管理のパイプラインの案件だけを表示しています。進行中の案件を先に、作成日の新しい順に並べています。応募・課金は掲載中の求人の直近 4 週の合計です。</p>
      {!deals && !error && <p role="status">案件を読み込んでいます…</p>}
      {deals && (deals.deals.length ? <div className="jh-scroll"><table className="jh-deal-table"><thead><tr><th scope="col">案件名</th><th scope="col">パイプライン</th><th scope="col">ステージ</th><th scope="col">作成日</th><th scope="col">契約期間</th><th scope="col" className="num">結び付いた求人</th><th scope="col" className="num">直近 4 週の応募</th><th scope="col" className="num">今の課金（4 週）</th></tr></thead><tbody>
        {deals.deals.map(deal => <tr key={deal.id} className={deal.active ? '' : 'jh-closed'}><th scope="row"><button type="button" className="jh-link" onClick={() => { onOpenDeal({ id: deal.id, name: dealName(deal) }); }}>{dealName(deal)} ›</button></th><td>{deal.pipeline_label}</td><td>{deal.stage_label ?? 'ステージ未取得'}{!deal.active && <small className="jh-badge">終了</small>}</td><td>{deal.created_at ?? '—'}</td><td>{contractLabel(deal)}</td><td className="num">{listingsLabel(deal)}</td><td className="num">{deal.recent_apps === null ? '—' : `${String(deal.recent_apps)} 件`}</td><td className="num">{yen(deal.recent_cost)}</td></tr>)}
      </tbody></table></div> : <p>この顧客に、納品管理の案件はありません。</p>)}
    </section>
    <CompanySummary company={company} billingByJob={billingByJob} onOpenListing={row => { void open(row); }} />
  </main>;
}
