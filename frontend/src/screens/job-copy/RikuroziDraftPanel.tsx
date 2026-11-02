import { useEffect, useRef, useState } from 'react';
import { apiGet, apiPost, apiPut, ApiHttpError, AuthRequiredError } from '../../api/client';
import { isHubSpotBusy, HUBSPOT_BUSY_MESSAGE } from './SnapshotErrorNotice';
interface Check { label: string; value: string | null; review: boolean; reason: string | null; missing_chunks: string[] }
export interface RikuroziDraft {
  regeneration_count?: number; review_required: boolean; threshold: number; min_sentence_chars: number; csv: string;
  comparisons: { title: string; media: string; publication: string; ratio: number | null; too_similar: boolean; whole_match: boolean; matched_sentences: string[] }[];
  copied: Check[]; generated: Check[]; images?: string[]; generated_values?: Record<string, string>;
}
interface CreatedListing { status: 'created' | 'existing' | 'skipped'; listing_id: string | null; linked_deals: number; unlinked_deals: number }
interface MadeListing { listing_id: string; name: string | null; reclogi_id: string | null }
function errorCode(error: unknown): string {
  const body = error instanceof ApiHttpError ? error.body : null;
  return typeof body === 'object' && body !== null && 'code' in body && typeof body.code === 'string' ? body.code : '';
}
const ID_ERRORS: Record<string, string> = {
  reclogi_id_invalid: '半角の数字だけで入力してください。',
  reclogi_id_in_use: 'このIDは、ほかの求人にすでに入っています。保存していません。',
  reclogi_id_already_set: 'この求人にはすでにIDが入っています。保存していません。',
  not_a_rikurozi_listing: 'リクロジ向けに作った求人ではないため、保存していません。',
};

/** リクロジ向けに作った求人のうち、リクロジの求人IDがまだ入っていないものに、人がIDを入れる。 */
function ReclogiIdField({ listingId, reload }: { listingId: string; reload: number }) {
  const [rows, setRows] = useState<MadeListing[]>([]);
  const [values, setValues] = useState<Record<string, string>>({});
  const [message, setMessage] = useState<Record<string, string>>({});
  useEffect(() => {
    const request = new AbortController();
    void apiGet<{ listings: MadeListing[] }>(`/api/job-copy/listings/${encodeURIComponent(listingId)}/rikurozi-listing`, { signal: request.signal }).then(result => {
      if (!request.signal.aborted && result.ok) setRows(Array.isArray(result.data.listings) ? result.data.listings : []);
    });
    return () => { request.abort(); };
  }, [listingId, reload]);
  async function save(row: MadeListing) {
    const result = await apiPut<{ reclogi_id: string }>(`/api/job-copy/rikurozi-listings/${encodeURIComponent(row.listing_id)}/reclogi-id`, { reclogi_id: (values[row.listing_id] ?? '').trim() });
    if (result.ok) {
      setRows(current => current.map(item => item.listing_id === row.listing_id ? { ...item, reclogi_id: result.data.reclogi_id } : item));
      setMessage(current => ({ ...current, [row.listing_id]: '' }));
    } else {
      setMessage(current => ({ ...current, [row.listing_id]: ID_ERRORS[errorCode(result.error)] ?? '保存できませんでした。少し待ってからもう一度お試しください。' }));
    }
  }
  if (rows.length === 0) return null;
  return <section aria-label="リクロジ向けに作った求人">
    <h4>リクロジ向けに作った求人</h4>
    {rows.map(row => <div key={row.listing_id}>
      <p>{row.name ?? '求人名なし'}</p>
      {row.reclogi_id
        ? <p>リクロジの求人ID：{row.reclogi_id}</p>
        : <form onSubmit={event => { event.preventDefault(); void save(row); }}>
          <label>リクロジの求人ID（リクロジに取り込んだあとで入力します）
            <input inputMode="numeric" value={values[row.listing_id] ?? ''} onChange={event => { setValues(current => ({ ...current, [row.listing_id]: event.target.value })); }} />
          </label>
          <button type="submit" className="jc-button" disabled={!(values[row.listing_id] ?? '').trim()}>IDを保存</button>
          {message[row.listing_id] && <p role="alert">{message[row.listing_id]}</p>}
        </form>}
    </div>)}
  </section>;
}

export function RikuroziDraftPanel({ listingId }: { listingId: string }) {
  const [draft, setDraft] = useState<RikuroziDraft | null>(null);
  const [busy, setBusy] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [exportMessage, setExportMessage] = useState('');
  const [reload, setReload] = useState(0);
  const [error, setError] = useState('');
  const controller = useRef<AbortController | null>(null);
  useEffect(() => () => { controller.current?.abort(); }, []);
  async function generate() {
    controller.current?.abort(); const request = new AbortController(); controller.current = request;
    setBusy(true); setError(''); setDraft(null); setExportMessage('');
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
  // 押したときに、HubSpotへ新しい求人を1件だけ作ってからCSVを出す。作れなければCSVは出さない。
  async function exportCsv() {
    if (!draft || exporting) return;
    setExporting(true); setExportMessage('');
    const result = await apiPost<CreatedListing>(`/api/job-copy/listings/${encodeURIComponent(listingId)}/rikurozi-listing`, { generated: draft.generated_values ?? {} }, { timeoutMs: 60_000 });
    setExporting(false);
    if (!result.ok) {
      const code = errorCode(result.error);
      setExportMessage(result.error instanceof AuthRequiredError ? 'Googleでログインしてから、もう一度押してください。'
        : isHubSpotBusy(result.error) ? HUBSPOT_BUSY_MESSAGE
        : code === 'deal_relation_missing' ? '元の求人に関連する取引がないため、求人を作っていません。取引との関連を確認してください。'
        : code === 'listing_title_missing' ? '案件名の案が空欄のため、求人を作っていません。案を作り直してください。'
        : 'HubSpotに求人を作れなかったため、CSVは出していません。少し待ってからもう一度押してください。');
      return;
    }
    const made = result.data;
    if (made.status === 'skipped') setExportMessage('いまはHubSpotへの登録を止めているため、求人は作らずにCSVだけを出しました。');
    else {
      const linked = `${String(made.linked_deals)}件の取引に関連付けました。`;
      setExportMessage(`${made.status === 'created' ? 'HubSpotに新しい求人（公開前）を作りました。' : 'すでに作ってある求人を使いました（新しくは作っていません）。'}${linked}${made.unlinked_deals > 0 ? `関連付けできなかった取引が${String(made.unlinked_deals)}件あります。もう一度押すと付け直します。` : ''}`);
      setReload(count => count + 1);
    }
    download();
  }
  return <section className="jc-rikurozi" aria-label="リクロジメディア向けの案">
    <button type="button" className="jc-button" disabled={busy} onClick={() => { void generate(); }}>リクロジメディア向けの案を作る</button>
    <p>選んだHRハッカーの求人を基準に、同じ取引先のAirWork求人と文章を比べます。</p>
    <p>新しい求人として取り込むCSVを作成します。画像は元の求人と同じ順序で最大3枚を使います。</p>
    <p>元の求人から取得できない連絡先・フォーム設定・公開設定などの項目は空欄で出力します。</p>
    {busy && <p role="status">本文と掲載状況を読み取り、項目ごとに文章の案を作成しています…</p>}
    {error && <p role="alert">{error}</p>}
    <ReclogiIdField listingId={listingId} reload={reload} />
    {draft && <>
      <h3>{draft.review_required ? '要確認の案' : '確認用の案'}</h3>
      <p>CSVを出す前に内容を確認してください。先方への送信・掲載は行いません。</p>
      <p>数字は原文に同じ表記があるかだけを確かめます。通常と研修の取り違えなど、正しい数字を違う文脈で使った誤りは検知できません。給与や時間の意味は担当者が確認してください。</p>
      <p>自動で作り直した回数：{String(draft.regeneration_count ?? 0)}回（最大2回）</p>
      <h4>比べた求人と文の一致率</h4>
      <p>文の一致率が{(draft.threshold * 100).toLocaleString('ja-JP', { maximumSignificantDigits: 6 })}％以上の場合、または生成した全文が相手の本文に含まれると「似すぎ・不合格」です。既知の項目見出しを除いて文を比べます。{draft.min_sentence_chars === 0 ? '短い文もすべて数えます。' : `${draft.min_sentence_chars.toLocaleString('ja-JP')}文字未満の文は一致率に数えません。`}</p>
      <ul>{draft.comparisons.map((item, index) => <li key={index}><strong>{item.media} · {item.title}</strong><span>{item.publication} ／ 文の一致率：{item.ratio === null ? '未取得（要確認）' : `${(item.ratio * 100).toFixed(1)}％`}{item.too_similar && ' ／ 似すぎ・不合格（要確認）'}</span>
        {item.whole_match && <p>生成した全文が相手の本文に含まれています。</p>}
        {item.matched_sentences.length > 0 && <><span>一致した文</span><ul>{item.matched_sentences.map((sentence, sentenceIndex) => <li key={sentenceIndex}>{sentence}</li>)}</ul></>}
      </li>)}</ul>
      <h4>原本の条件</h4>
      <p>勤務地は基準の事業所を引き継ぎます。詳しい住所は事業所の情報で確認してください。</p>
      <dl>{draft.copied.map(item => <div key={item.label}><dt>{item.label}{item.review && '（要確認）'}</dt><dd>{item.value ?? '未取得'}{item.reason && <p>{item.reason}</p>}</dd></div>)}</dl>
      {draft.images && <>
        <h4>使用する画像</h4>
        <ol>{draft.images.slice(0, 3).map((url, index) => <li key={index}>画像{index + 1}：{url || '空欄'}</li>)}</ol>
      </>}
      <h4>作成した文章と確認結果</h4>
      <dl>{draft.generated.map(item => <div key={item.label}><dt>{item.label}{item.review && '（要確認）'}</dt><dd>{item.value ?? '未取得'}{item.reason && <p>{item.reason}</p>}{item.missing_chunks.length > 0 && <><p>原文にない数字の表記</p><ul>{item.missing_chunks.map(chunk => <li key={chunk}>{chunk}</li>)}</ul></>}</dd></div>)}</dl>
      {draft.review_required && <p role="status">要確認の項目があります。原文にない数字の表記がある文章は空欄です。似すぎの文章は作り直すか、担当者が確認してください。</p>}
      <button type="button" className="jc-button" disabled={exporting} onClick={() => { void exportCsv(); }}>この案でCSVを出す</button>
      <p>押すと、HubSpotに新しい求人（公開前）を1件作り、元の求人と同じ取引に関連付けてから、CSVを出します。同じ求人で何度押しても、作るのは1件だけです。</p>
      {exportMessage && <p role="status">{exportMessage}</p>}
    </>}
  </section>;
}
