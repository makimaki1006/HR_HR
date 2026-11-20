import { useEffect, useState } from 'react';
import type { ApplicantCalls } from '../../generated/ApplicantCalls';
import type { ApplicantCall } from '../../generated/ApplicantCall';

/** 畳まずに見せる件数。残りは「ほかの発信」に畳む */
const SHOWN = 3;
const FAILED = '発信の履歴を読めませんでした。';

/** 架空サンプル (mode=fixture) 用。実在の人・番号ではない */
export const applicantCallsFixture: ApplicantCalls = {
  days: 60,
  calls: [
    { at: '2026-10-17T15:42:00+09:00', caller: '架空 みなと', answered: true, seconds: 204 },
    { at: '2026-10-17T11:05:00+09:00', caller: '架空 みなと', answered: false, seconds: 0 },
    { at: '2026-10-16T18:20:00+09:00', caller: '', answered: false, seconds: 0 },
    { at: '2026-10-15T10:12:00+09:00', caller: '架空 つばさ', answered: true, seconds: 47 },
  ],
};

function when(raw: string): string {
  const value = new Date(raw);
  return Number.isNaN(value.getTime()) ? raw : new Intl.DateTimeFormat('ja-JP', { timeZone: 'Asia/Tokyo', month: 'numeric', day: 'numeric', weekday: 'short', hour: '2-digit', minute: '2-digit' }).format(value);
}
function length(call: ApplicantCall): string {
  if (!call.answered) return 'つながらず';
  const m = Math.floor(call.seconds / 60);
  const s = call.seconds % 60;
  return `通話 ${m > 0 ? `${String(m)}分` : ''}${String(s)}秒`;
}
function Rows({ calls }: { calls: ApplicantCall[] }) {
  return <ul className="applicant-calls-list">{calls.map((c, i) => <li key={`${c.at}-${String(i)}`}>
    <time dateTime={c.at}>{when(c.at)}</time><span>{c.caller || 'かけた人は不明'}</span><span className={c.answered ? 'applicant-calls-talked' : undefined}>{length(c)}</span>
  </li>)}</ul>;
}

/** 応募者の番号に社内の誰かがかけた履歴 (Zoom Phone)。読めなくても応募のほかの表示は止めない */
export function ApplicantCallHistory({ applicantId, fixture }: { applicantId: string; fixture: boolean }) {
  const [state, setState] = useState<{ id: string; data: ApplicantCalls | null; failed: boolean }>({ id: applicantId, data: null, failed: false });
  const [retry, setRetry] = useState(0);
  useEffect(() => {
    if (fixture) return;
    const abort = new AbortController();
    void fetch(`/api/crm/applicants/${encodeURIComponent(applicantId)}/calls`, { signal: abort.signal, credentials: 'same-origin' })
      .then(async r => {
        const body = r.ok ? await r.json() as Partial<ApplicantCalls> | null : null;
        if (!body || !Array.isArray(body.calls) || typeof body.days !== 'number') throw new Error('failed');
        return { days: body.days, calls: body.calls };
      })
      .then(data => { if (!abort.signal.aborted) setState({ id: applicantId, data, failed: false }); })
      .catch(() => { if (!abort.signal.aborted) setState({ id: applicantId, data: null, failed: true }); });
    return () => { abort.abort(); };
  }, [applicantId, fixture, retry]);
  const current = state.id === applicantId ? state : { id: applicantId, data: null, failed: false };
  const data = fixture ? applicantCallsFixture : current.data;
  return <section className="applicant-calls" aria-label="この番号への発信">
    <h3>この番号への発信{data && <span>直近{data.days}日 · {data.calls.length}件</span>}</h3>
    {!data && !current.failed && <p role="status">読み込み中…</p>}
    {!data && current.failed && <p role="status">{FAILED}<button type="button" onClick={() => { setState({ id: applicantId, data: null, failed: false }); setRetry(v => v + 1); }}>再試行</button></p>}
    {data?.calls.length === 0 && <p>直近{data.days}日、社内からこの番号への発信はありません。</p>}
    {data && data.calls.length > 0 && <>
      <Rows calls={data.calls.slice(0, SHOWN)} />
      {data.calls.length > SHOWN && <details><summary>ほかの発信 {data.calls.length - SHOWN} 件</summary><Rows calls={data.calls.slice(SHOWN)} /></details>}
    </>}
  </section>;
}
