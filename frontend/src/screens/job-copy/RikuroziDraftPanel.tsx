import { useEffect, useRef, useState } from 'react';
import { apiPost, AuthRequiredError } from '../../api/client';
import { isHubSpotBusy, HUBSPOT_BUSY_MESSAGE } from './SnapshotErrorNotice';
interface Check { label: string; value: string | null; review: boolean; reason: string | null }
export interface RikuroziDraft {
  review_required: boolean; threshold: number; csv: string;
  comparisons: { title: string; media: string; publication: string; ratio: number | null; too_similar: boolean }[];
  copied: Check[]; generated: Check[];
}
export function RikuroziDraftPanel({ listingId }: { listingId: string }) {
  const [draft, setDraft] = useState<RikuroziDraft | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const controller = useRef<AbortController | null>(null);
  useEffect(() => () => { controller.current?.abort(); }, []);
  async function generate() {
    controller.current?.abort(); const request = new AbortController(); controller.current = request;
    setBusy(true); setError(''); setDraft(null);
    const result = await apiPost<RikuroziDraft>(`/api/job-copy/listings/${encodeURIComponent(listingId)}/rikurozi-draft`, {}, { signal: request.signal, timeoutMs: 300_000 });
    if (request.signal.aborted) return;
    if (result.ok) setDraft(result.data);
    else setError(result.error instanceof AuthRequiredError ? 'Googleでログインしてから作り直してください。' : isHubSpotBusy(result.error) ? HUBSPOT_BUSY_MESSAGE : '案を作成できませんでした。取引先との関連、求人の本文・事業所の情報を確認してから作り直してください。');
    setBusy(false);
  }
  function download() {
    if (!draft) return;
    const url = URL.createObjectURL(new Blob([draft.csv], { type: 'text/csv;charset=utf-8' }));
    const link = document.createElement('a'); link.href = url; link.download = 'リクロジメディア_確認用の案.csv'; link.click();
    window.setTimeout(() => { URL.revokeObjectURL(url); }, 1000);
  }
  return <section className="jc-rikurozi" aria-label="リクロジメディア向けの案">
    <button type="button" className="jc-button" disabled={busy} onClick={() => { void generate(); }}>リクロジメディア向けの案を作る</button>
    <p>選んだHRハッカーの求人を基準に、同じ取引先のAirWork求人と文章を比べます。</p>
    {busy && <p role="status">本文と掲載状況を読み取り、案を作成しています…</p>}
    {error && <p role="alert">{error}</p>}
    {draft && <>
      <h3>{draft.review_required ? '要確認の案' : '確認用の案'}</h3>
      <p>ダウンロード前に内容を確認してください。先方への送信・掲載は行いません。</p>
      <h4>比べた求人と文章の重なり</h4>
      <p>文字の並びの重なりが{(draft.threshold * 100).toLocaleString('ja-JP', { maximumSignificantDigits: 6 })}％以上なら「似すぎ」です。条件の共通点も含まれるため、内容を確認してください。</p>
      <ul>{draft.comparisons.map((item, index) => <li key={index}><strong>{item.media} · {item.title}</strong><span>{item.publication} ／ 重なり：{item.ratio === null ? '未取得（要確認）' : `${(item.ratio * 100).toFixed(1)}％`}{item.too_similar && ' ／ 似すぎ（要確認）'}</span></li>)}</ul>
      <h4>原本から写した条件</h4>
      <p>勤務地は基準の事業所を引き継ぎます。詳しい住所は事業所の情報で確認してください。</p>
      <dl>{draft.copied.map(item => <div key={item.label}><dt>{item.label}{item.review && '（要確認）'}</dt><dd>{item.value ?? '未取得'}{item.reason && <p>{item.reason}</p>}</dd></div>)}</dl>
      <h4>作成した文章と確認結果</h4>
      <dl>{draft.generated.map(item => <div key={item.label}><dt>{item.label}{item.review && '（要確認）'}</dt><dd>{item.value ?? '未取得'}{item.reason && <p>{item.reason}</p>}</dd></div>)}</dl>
      {draft.review_required && <p role="status">要確認の項目があります。数字が一致しない文章は空欄です。似すぎの文章は作り直すか、担当者が確認してください。</p>}
      <button type="button" className="jc-button" onClick={download}>確認用の案をダウンロード</button>
    </>}
  </section>;
}
