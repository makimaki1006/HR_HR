import { useEffect, useRef, useState } from 'react';
import { apiPost, ApiHttpError, AuthRequiredError } from '../../api/client';
import { isHubSpotBusy, HUBSPOT_BUSY_MESSAGE } from './SnapshotErrorNotice';
interface Check { label: string; value: string | null; review: boolean; reason: string | null; missing_chunks: string[] }
export interface RikuroziDraft {
  review_required: boolean; threshold: number; min_sentence_chars: number; csv: string;
  comparisons: { title: string; media: string; publication: string; ratio: number | null; too_similar: boolean; whole_match: boolean; matched_sentences: string[] }[];
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
    else {
      const copyMismatch = result.error instanceof ApiHttpError && typeof result.error.body === 'object' && result.error.body !== null && 'code' in result.error.body && result.error.body.code === 'draft_copy_mismatch';
      setError(copyMismatch ? '原本から写した条件が一致しないため、出力を止めました（要確認）。担当者が原本の内容を確認してください。' : result.error instanceof AuthRequiredError ? 'Googleでログインしてから作り直してください。' : isHubSpotBusy(result.error) ? HUBSPOT_BUSY_MESSAGE : '案を作成できませんでした（要確認）。取引先との関連、求人の本文・事業所の情報を確認してから作り直してください。');
    }
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
      <p>数字は原文に同じ表記があるかだけを確かめます。通常と研修の取り違えなど、正しい数字を違う文脈で使った誤りは検知できません。給与や時間の意味は担当者が確認してください。</p>
      <h4>比べた求人と文の一致率</h4>
      <p>文の一致率が{(draft.threshold * 100).toLocaleString('ja-JP', { maximumSignificantDigits: 6 })}％以上、または生成した全文が相手の本文に含まれると「似すぎ・不合格」です。初期値は仮置きです。{draft.min_sentence_chars === 0 ? '短い文もすべて数えます。' : `${draft.min_sentence_chars.toLocaleString('ja-JP')}文字未満の文は一致率に数えません。`}</p>
      <ul>{draft.comparisons.map((item, index) => <li key={index}><strong>{item.media} · {item.title}</strong><span>{item.publication} ／ 文の一致率：{item.ratio === null ? '未取得（要確認）' : `${(item.ratio * 100).toFixed(1)}％`}{item.too_similar && ' ／ 似すぎ・不合格（要確認）'}</span>
        {item.whole_match && <p>生成した全文が相手の本文に含まれています。</p>}
        {item.matched_sentences.length > 0 && <><span>一致した文</span><ul>{item.matched_sentences.map((sentence, sentenceIndex) => <li key={sentenceIndex}>{sentence}</li>)}</ul></>}
      </li>)}</ul>
      <h4>原本から写した条件</h4>
      <p>勤務地は基準の事業所を引き継ぎます。詳しい住所は事業所の情報で確認してください。</p>
      <dl>{draft.copied.map(item => <div key={item.label}><dt>{item.label}{item.review && '（要確認）'}</dt><dd>{item.value ?? '未取得'}{item.reason && <p>{item.reason}</p>}</dd></div>)}</dl>
      <h4>作成した文章と確認結果</h4>
      <dl>{draft.generated.map(item => <div key={item.label}><dt>{item.label}{item.review && '（要確認）'}</dt><dd>{item.value ?? '未取得'}{item.reason && <p>{item.reason}</p>}{item.missing_chunks.length > 0 && <><p>原文にない数字の表記</p><ul>{item.missing_chunks.map(chunk => <li key={chunk}>{chunk}</li>)}</ul></>}</dd></div>)}</dl>
      {draft.review_required && <p role="status">要確認の項目があります。原文にない数字の表記がある文章は空欄です。似すぎの文章は作り直すか、担当者が確認してください。</p>}
      <button type="button" className="jc-button" onClick={download}>確認用の案をダウンロード</button>
    </>}
  </section>;
}
