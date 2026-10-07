import { useRef, useState } from 'react';
import { apiGet } from '../../api/client';
import type { JobCopyRecord } from './data';
import { parseMediaCapture } from './mediaCaptureParser';
import { parseApplicantReasons } from './applicantReasonsParser';
import type { ApplicantDimension } from './applicantCompositionModel';
import { roundAreaCounts, roundApplicantAreasInRecord } from './applicantArea';

interface RecordData { id: string; properties: Record<string, string | null> }
interface CustomerPage { customers: RecordData[]; next_after: string | null; total_ms: number }
interface JobPage { company_id: string; portal_id?: string | null; contracts: RecordData[]; jobs: { record: RecordData; deal_ids: string[] }[]; total: number; next_offset: number | null; total_ms: number; fetched_at: string }
interface Summary { total: number; duplicate_ids: number; missing_date: number; by_date: Record<string, number>; dimensions: Record<string, Record<string, number>> }
interface DatedComparison { total: number; unknown: number; basis: string; daily_representatives?: Record<string, { version_id: string }>; by_version: Record<string, { count: number; dimensions: Record<ApplicantDimension, { denominator: number; categories: { category: string; count: number; percentage: number | null }[] } | null> }> }
interface ApplicantPage { metric: string; summary: Summary; total_ms: number; fetched_at: string; version_attribution: string; attribute_basis: string; capture_bundle?: unknown; dated_comparison?: DatedComparison | null; capture_status?: string; applicant_reasons?: unknown }
const labels: Record<string, string> = { gender: '性別', age: '年代', prefecture: '都道府県', municipality: '市区町村' };
const requestOptions = { timeoutMs: 120_000 };

export function HubSpotReadPanel({ onOpen }: { onOpen: (job: JobCopyRecord) => void }) {
  const [customers, setCustomers] = useState<RecordData[]>([]);
  const [after, setAfter] = useState<string | null>(null);
  const [customer, setCustomer] = useState('');
  const [search, setSearch] = useState('');
  const [contract, setContract] = useState('');
  const [page, setPage] = useState<JobPage | null>(null);
  const [applications, setApplications] = useState<ApplicantPage | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState('');
  const [error, setError] = useState('');
  const generation = useRef(0);
  function failure(status: string) { setError(`HubSpotの読み取りに失敗しました（${status}）。Googleログイン・求人管理の閲覧権限・サーバー設定を確認してください。`); }
  async function loadCustomers(more = false) {
    const request = ++generation.current;
    setBusy(true); setError('');
    const result = await apiGet<CustomerPage>(`/api/job-copy/live${more && after ? `?after=${encodeURIComponent(after)}` : ''}`, requestOptions);
    if (request !== generation.current) return;
    if (result.ok) {
      setCustomers(previous => more ? [...new Map([...previous, ...result.data.customers].map(row => [row.id, row])).values()] : result.data.customers);
      setAfter(result.data.next_after); setMessage(`取引先読み取り ${String(Math.round(result.data.total_ms))}ms`);
    } else failure(result.error.message);
    setBusy(false);
  }
  async function loadJobs(offset = 0) {
    if (!customer) return;
    const request = ++generation.current;
    setBusy(true); setError(''); setApplications(null);
    const result = await apiGet<JobPage>(`/api/job-copy/live?company=${encodeURIComponent(customer)}&offset=${String(offset)}`, requestOptions);
    if (request !== generation.current) return;
    if (result.ok) { setPage(result.data); setContract(''); setMessage(`求人読み取り ${String(Math.round(result.data.total_ms))}ms`); }
    else failure(result.error.message);
    setBusy(false);
  }
  async function open(record: RecordData) {
    if (!page) return;
    const request = ++generation.current;
    setBusy(true); setError(''); setApplications(null);
    const name = customers.find(row => row.id === page.company_id)?.properties.name ?? '取引先名未取得';
    const body = record.properties.shigotonaiyou;
    const selectedJob: JobCopyRecord = { id: `hubspot-${record.id}`, title: record.properties.hs_name ?? '求人名未取得', company: name,
      media: record.properties.id_hrhakkaa ? 'HRハッカー' : record.properties.id_airwork ? 'AirWork' : '媒体未対応',
      mediaJobId: record.properties.id_hrhakkaa ?? record.properties.id_airwork ?? '', location: record.properties.qinwude ?? '',
      hubspotId: record.id, dataSource: 'hubspot',
      ...(page.portal_id ? { hubspotUrl: `https://app.hubspot.com/contacts/${page.portal_id}/record/0-420/${record.id}` } : {}),
      versions: body ? [{ id: `hubspot-${record.id}-${page.fetched_at}`, label: 'HubSpotの現在の仕事内容', observedAt: page.fetched_at,
        kind: 'received', certainty: 'unknown', source: 'HubSpot shigotonaiyou', body, applications: null,
        note: 'HubSpotの現在値です。媒体の求人票全文・日次履歴・画像はまだ接続していません。取得日を掲載変更日として扱いません。' }] : [],
    };
    onOpen(selectedJob);
    const result = await apiGet<ApplicantPage>(`/api/job-copy/live?company=${encodeURIComponent(page.company_id)}&listing=${encodeURIComponent(record.id)}`, requestOptions);
    if (request !== generation.current) return;
    if (result.ok) {
      setApplications(result.data); setMessage(`応募読み取り ${String(Math.round(result.data.total_ms))}ms`);
      if (result.data.capture_bundle && result.data.dated_comparison) {
        try {
          const captured = parseMediaCapture(JSON.stringify(result.data.capture_bundle))[0];
          const comparison = result.data.dated_comparison;
          if (captured) onOpen(roundApplicantAreasInRecord({ ...captured, id: selectedJob.id, company: selectedJob.company, hubspotId: record.id, ...(selectedJob.hubspotUrl ? { hubspotUrl: selectedJob.hubspotUrl } : {}), dataSource: 'hubspot', attributionUnknown: comparison.unknown,
            applicantReasons: parseApplicantReasons(result.data.applicant_reasons, result.data.summary.total, captured.versions.filter(version => version.kind === 'published').map(version => version.id)),
            versions: captured.versions.map(version => {
              const bucket = comparison.by_version[version.id];
              if (!bucket) return version;
              return { ...version, certainty: 'estimated', applications: { confirmed: 0, estimated: bucket.count, unknown: 0 },
                observationDates: Object.entries(comparison.daily_representatives ?? {}).filter(([, day]) => day.version_id === version.id).map(([date]) => date).sort(),
                attributesFetchedAt: result.data.fetched_at,
                distributions: Object.fromEntries(Object.entries(bucket.dimensions).filter(([, distribution]) => distribution !== null).map(([dimension, distribution]) => [dimension, { total: distribution?.denominator ?? 0, categories: distribution?.categories ?? [] }])),
                note: `${version.note} ${comparison.basis}` };
            }),
          }));
        } catch { setError('媒体観測データの形式を検証できませんでした。HubSpotの現在値と応募全体の集計を表示します。'); }
      } else {
        try { onOpen({ ...selectedJob, applicantReasons: parseApplicantReasons(result.data.applicant_reasons, result.data.summary.total, []) }); }
        catch { setError('応募理由の出典・件数を確認できませんでした。原記録を推測して補完しません。'); }
      }
    }
    else failure(result.error.message);
    setBusy(false);
  }
  return <details className="jc-live-panel"><summary>HubSpotの取引先・求人・応募を確認</summary>
    <p>既存の求人連携を読み取ります。更新・関連付けの変更は行いません。</p>
    <button className="jc-button" disabled={busy} onClick={() => { void loadCustomers(); }}>取引先を取得</button>
    {after && <button className="jc-button" disabled={busy} onClick={() => { void loadCustomers(true); }}>取引先をさらに取得</button>}
    <label>取得済みの取引先を検索<input value={search} onChange={event => { setSearch(event.target.value); }} /></label>
    <label>HubSpotの取引先<select disabled={busy} value={customer} onChange={event => { ++generation.current; setCustomer(event.target.value); setPage(null); setApplications(null); }}><option value="">取引先を選択</option>{customers.filter(row => (row.properties.name ?? '').includes(search)).map(row => <option key={row.id} value={row.id}>{row.properties.name ?? '名称未取得'}</option>)}</select></label>
    <button className="jc-button" disabled={busy || !customer} onClick={() => { void loadJobs(); }}>関連する求人を取得</button>
    {busy && <p role="status">HubSpotを読み取り中…</p>}{message && <p role="status">{message}</p>}{error && <p className="jc-error" role="alert">{error}</p>}
    {page && <><p>関連する求人は{page.total}件。このページは{page.jobs.length}件です。現契約・旧契約を含む現在の関連を使用しています。</p>
      <label>契約で絞り込む<select value={contract} onChange={event => { setContract(event.target.value); }}><option value="">このページの全契約</option>{page.contracts.map(row => <option key={row.id} value={row.id}>{row.properties.dealname ?? row.id} / {row.properties.code_of_customer ?? 'コード未取得'}</option>)}</select></label>
      <div className="jc-live-jobs">{page.jobs.filter(job => !contract || job.deal_ids.includes(contract)).map(({ record, deal_ids }) => <button className="jc-button" key={record.id} disabled={busy} onClick={() => { void open(record); }}>{record.properties.hs_name ?? '求人名未取得'}（関連契約{deal_ids.length}件）</button>)}</div>
      {page.next_offset !== null && <button className="jc-button" disabled={busy} onClick={() => { void loadJobs(page.next_offset ?? 0); }}>次の20件</button>}
    </>}
    {applications && <section aria-label="実応募の読み取り結果"><h2>{applications.metric}: {applications.summary.total}件</h2><p>{applications.version_attribution}</p><p>{applications.attribute_basis} · 応募日不明{applications.summary.missing_date}件 · 取得{applications.fetched_at}</p>
      <p>{applications.dated_comparison ? `${applications.dated_comparison.basis} 版対応不明${String(applications.dated_comparison.unknown)}件。本文・履歴と応募者構成のタブで確認できます。` : '日付ごとの観測版が未接続のため、ここでは求人全体の構成を表示します。'}</p>
      <details><summary>求人全体の応募属性を開く</summary>{Object.entries(applications.summary.dimensions).map(([dimension, buckets]) => <section key={dimension}><h3>{labels[dimension] ?? dimension}</h3><table><thead><tr><th scope="col">区分</th><th scope="col">件数</th><th scope="col">割合</th></tr></thead><tbody>{Object.entries(dimension === 'prefecture' || dimension === 'municipality' ? roundAreaCounts(dimension, buckets) : buckets).map(([label, count]) => <tr key={label}><th scope="row">{label}</th><td>{count}件</td><td>{applications.summary.total ? `${(100 * count / applications.summary.total).toFixed(1)}%` : '算出不可'}</td></tr>)}</tbody></table></section>)}</details>
    </section>}
  </details>;
}
