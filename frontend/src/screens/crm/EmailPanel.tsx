import { useEffect, useId, useRef, useState } from 'react';
import type { WorkspaceResponse } from '../../generated/WorkspaceResponse';
import type { SendEmailRequest } from '../../generated/SendEmailRequest';
import type { EmailResult } from '../../generated/EmailResult';
import type { GmailGrantStatus } from '../../generated/GmailGrantStatus';
import { ApiHttpError } from '../../api/client';
import { allowFakeGmail, emailChanged, emailMessage, fakeEmailApi, liveEmailApi, resultFromError, textHtml } from './emailApi';
import type { EmailApi } from './emailApi';
import './email.css';

export function EmailPanel({ data, sample = false, api: suppliedApi, onSaved }: {
  data: WorkspaceResponse; sample?: boolean; api?: EmailApi; onSaved?: (() => void) | undefined;
}) {
  const formId = useId();
  const enabled = suppliedApi !== undefined || document.getElementById('app-root')?.dataset.crmEmailEnabled === '1';
  const api = suppliedApi ?? (sample ? fakeEmailApi : liveEmailApi);
  const [grant, setGrant] = useState<GmailGrantStatus | null>(null);
  const [open, setOpen] = useState(false);
  const [contactId, setContactId] = useState('');
  const [cc, setCc] = useState<string[]>([]);
  const [subject, setSubject] = useState('');
  const [body, setBody] = useState('');
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<EmailResult | null>(null);
  const [message, setMessage] = useState('');
  // Never assign another ID on a timeout; retry/status confirmation keeps the original request.
  const polling = useRef({ delay: 5000, elapsed: 0 });
  const [attempted, setAttempted] = useState<SendEmailRequest | null>(null);
  const contacts = data.contacts.filter(c => c.email !== null && c.email !== '');
  const contact = contacts.find(c => c.id === contactId) ?? contacts[0];
  useEffect(() => {
    if (!enabled) return;
    let active = true;
    void api.grant().then(r => { if (active && r.ok) setGrant(r.data); });
    return () => { active = false; };
  }, [api, enabled]);
  useEffect(() => {
    if (attempted === null || result?.status !== 'pending' || polling.current.elapsed > 30 * 60 * 1000) return;
    let active = true;
    const timer = setTimeout(() => {
      void api.status(attempted.operation_id).then(r => {
        if (!active || !r.ok) return;
        setResult(r.data);
        if (r.data.status === 'saved') {
          setAttempted(null); setSubject(''); setBody(''); setOpen(false);
          emailChanged(data.deal.id); onSaved?.();
        }
      });
    }, polling.current.delay);
    polling.current.elapsed += polling.current.delay;
    polling.current.delay = Math.min(polling.current.delay * 2, 60000);
    return () => { active = false; clearTimeout(timer); };
  }, [api, attempted, result, data.deal.id, onSaved]);
  async function revoke() {
    setBusy(true);
    const r = await api.revoke();
    setBusy(false);
    if (r.ok) { setGrant({ enabled: true, granted: false }); setMessage(r.data.google_revocation_pending ? 'この画面からの送信許可を取り消しました。Google アカウントの「接続」からも許可を取り消してください。' : '送信許可を取り消しました。'); }
    else setMessage('許可を取り消せませんでした。時間をおいてもう一度お試しください。');
  }
  async function send() {
    if (busy || contact?.email == null || subject.trim() === '' || body.trim() === '') return;
    const req = attempted ?? { operation_id: crypto.randomUUID(), hubspot_deal_id: data.deal.id, contact_id: contact.id, to: contact.email, cc, subject, html: textHtml(body) };
    setAttempted(req);
    polling.current = { delay: 5000, elapsed: 0 };
    setBusy(true); setMessage('');
    const res = await api.send(req);
    setBusy(false);
    const saved = res.ok ? res.data : resultFromError(res.error);
    if (saved) {
      setResult(saved); emailChanged(data.deal.id);
      if (saved.status === 'saved') { setAttempted(null); setSubject(''); setBody(''); setOpen(false); onSaved?.(); }
      return;
    }
    if (!res.ok && res.error instanceof ApiHttpError && res.error.status === 403) {
      const kind = (res.error.body as { error_kind?: string } | undefined)?.error_kind;
      if (kind === 'gmail_grant_required') { setGrant({ enabled: true, granted: false }); setAttempted(null); setMessage('Gmail での送信を許可してください'); }
      else { setAttempted(null); setMessage('選んだ案件の担当者にだけ送信できます。最新の情報を確認してください。'); }
    } else if (!res.ok && res.error instanceof ApiHttpError && (res.error.status === 422 || res.error.status === 429)) {
      setAttempted(null); setMessage('送信できませんでした。入力内容を確認し、時間をおいてお試しください。');
    } else setMessage('送信結果を確認できません。「送信結果を確認」を押してください。');
  }
  async function check() {
    const req = attempted;
    if (!req) return;
    setBusy(true);
    const r = await api.status(req.operation_id);
    setBusy(false);
    if (r.ok) { setResult(r.data); setMessage(''); emailChanged(data.deal.id); if (r.data.status === 'saved') { setAttempted(null); setSubject(''); setBody(''); setOpen(false); onSaved?.(); } }
    else if (r.error instanceof ApiHttpError && r.error.status === 404) { setAttempted(null); setMessage('送信は受け付けられていません。入力内容を確認して送信してください。'); }
    else setMessage('送信結果を確認できませんでした。時間をおいてもう一度ご確認ください。');
  }
  if (grant?.enabled !== true) return null;
  return <section className="crm-email" aria-label="メール送信">
    <div className="crm-email-actions">
      <button type="button" disabled={busy || attempted !== null} onClick={() => { setOpen(v => !v); }}>メールを書く</button>
      {grant.granted && <button type="button" disabled={busy} onClick={() => { void revoke(); }}>Gmail の許可を取り消す</button>}
    </div>
    {!grant.granted && <div className="crm-email-permission"><p>Gmail での送信を許可してください</p>
      {sample ? <button type="button" onClick={() => { allowFakeGmail(); setGrant({ enabled: true, granted: true }); }}>Gmail で送信を許可する</button>
        : <a href="/api/crm/gmail/authorize">Gmail で送信を許可する</a>}
    </div>}
    {result && <p role="status" className={`crm-email-state crm-email-${result.status}`}>{emailMessage(result)}</p>}
    {message && <p role="alert" className="crm-email-state crm-email-failed">{message}</p>}
    {attempted && result?.status !== 'saved' && <button type="button" disabled={busy} onClick={() => { void check(); }}>送信結果を確認</button>}
    {open && grant.granted && <form aria-label="メールを書く" onSubmit={e => { e.preventDefault(); void send(); }}>
      <fieldset disabled={busy || attempted !== null}>
        <label htmlFor={`${formId}-to`}>宛先</label><select id={`${formId}-to`} value={contact?.id ?? ''} onChange={e => { setContactId(e.target.value); }} required>
          {contacts.map(c => <option key={c.id} value={c.id}>{c.name ?? '担当者'} — {c.email}</option>)}
        </select>
        <fieldset className="crm-email-cc"><legend>CC（この案件の担当者）</legend>
          {contacts.filter(c => c.id !== contact?.id).map(c => <label key={c.id}><input type="checkbox" checked={cc.includes(c.email ?? '')} onChange={e => { const addr = c.email ?? ''; setCc(old => e.target.checked ? [...old, addr] : old.filter(a => a !== addr)); }} />{c.name} — {c.email}</label>)}
          {contacts.length < 2 && <small>選べる担当者はいません</small>}
        </fieldset>
        <label>件名<input value={subject} onChange={e => { setSubject(e.target.value); }} maxLength={512} required /></label>
        <label>本文<textarea value={body} onChange={e => { setBody(e.target.value); }} rows={7} maxLength={30000} required /></label>
      </fieldset>
      <div className="crm-email-actions"><button type="submit" disabled={busy || attempted !== null || !contact || !subject.trim() || !body.trim()}>{busy ? '送信中…' : '送信'}</button>
        <button type="button" disabled={busy} onClick={() => { setOpen(false); }}>閉じる</button></div>
    </form>}
  </section>;
}
