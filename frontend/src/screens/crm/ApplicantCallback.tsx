import { useState } from 'react';
import type { ApplicantRow } from '../../generated/ApplicantRow';
import './write.css';

const HEADERS = { 'Content-Type': 'application/json', 'X-Requested-With': 'fetch' };
const MEMO_MAX = 1000;
interface Result { kind: 'ok' | 'error'; text: string }

const SENT = 'Slack に知らせました。';
const FAILED = '送れませんでした。何も送られていません。もう一度お試しください。';
const UNKNOWN = '送れたか確かめられませんでした。Slack を見て、届いていなければもう一度送ってください。';

function newId() { return typeof crypto.randomUUID === 'function' ? crypto.randomUUID() : String(Math.random()).slice(2); }

async function send(row: ApplicantRow, memo: string, operationId: string, fixture: boolean): Promise<Result> {
  if (fixture) {
    // 架空サンプルでは送らない。内容に「失敗」を含めると、送れなかったときの表示を確かめられる。
    await new Promise(resolve => setTimeout(resolve, 300));
    return memo.includes('失敗') ? { kind: 'error', text: FAILED } : { kind: 'ok', text: SENT };
  }
  try {
    const res = await fetch(`/api/crm/applicants/${encodeURIComponent(row.id)}/callback`, {
      method: 'POST', credentials: 'same-origin', headers: HEADERS,
      body: JSON.stringify({ operation_id: operationId, memo, name: row.name, deal: row.deal, job: row.job }),
    });
    const body = await res.json().catch(() => null) as { result?: string } | null;
    if (res.ok && body?.result === 'sent') return { kind: 'ok', text: SENT };
    if (body?.result === 'not_configured') return { kind: 'error', text: '送り先がまだ設定されていないため、送れませんでした。何も送られていません。' };
    if (body?.result === 'sending') return { kind: 'error', text: '同じ内容を送っている途中です。少し待って Slack を確かめてください。' };
    if (body?.result === 'unknown') return { kind: 'error', text: UNKNOWN };
    if (res.status === 401 || res.status === 403) return { kind: 'error', text: '送れませんでした。何も送られていません。ログインし直してからお試しください。' };
    return { kind: 'error', text: FAILED };
  } catch {
    return { kind: 'error', text: UNKNOWN };
  }
}

/** 応募者からの折り返しの電話を Slack に知らせる。応募ごとに `key` を付けて使う (応募を切り替えたら入力を捨てる)。 */
export function ApplicantCallback({ row, enabled, fixture }: { row: ApplicantRow; enabled: boolean; fixture: boolean }) {
  const [open, setOpen] = useState(false);
  const [memo, setMemo] = useState('');
  const [operationId, setOperationId] = useState('');
  const [sending, setSending] = useState(false);
  const [result, setResult] = useState<Result | null>(null);
  if (!enabled) return <p className="applicant-callback"><button type="button" disabled>折り返しを知らせる</button><span className="applicant-callback-note">Slack の送り先がまだ設定されていないため使えません。</span></p>;
  function submit() {
    if (sending) return;
    setSending(true);
    setResult(null);
    void send(row, memo.trim(), operationId, fixture).then(r => {
      setSending(false);
      setResult(r);
      if (r.kind === 'ok') { setOpen(false); setMemo(''); }
    });
  }
  return <div className="applicant-callback">
    {!open && <button type="button" onClick={() => { setOperationId(newId()); setResult(null); setOpen(true); }}>折り返しを知らせる</button>}
    {open && <form onSubmit={e => { e.preventDefault(); submit(); }}>
      <label>折り返しの内容（Slack に送ります）
        <textarea value={memo} maxLength={MEMO_MAX} rows={3} autoFocus disabled={sending} placeholder="例: 14時ごろ折り返しあり。明日の午前にかけ直してほしいとのこと" onChange={e => { setMemo(e.target.value); }} />
      </label>
      <div className="applicant-callback-actions">
        <button type="submit" disabled={sending}>{sending ? '送っています…' : 'Slack に送る'}</button>
        <button type="button" disabled={sending} onClick={() => { setOpen(false); setResult(null); }}>やめる</button>
      </div>
    </form>}
    {result && <p className={`wr-status ${result.kind === 'ok' ? 'wr-saved' : 'wr-error'}`} role={result.kind === 'ok' ? 'status' : 'alert'}>{result.text}</p>}
  </div>;
}
