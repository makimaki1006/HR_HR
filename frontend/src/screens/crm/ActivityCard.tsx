import { memo, useMemo, useState } from 'react';
import type { WorkspaceActivity } from '../../generated/WorkspaceActivity';
import {
  htmlToText, previewLines, sanitizeHtml, splitQuotedHtml, splitQuotedText,
} from './activityHtml';
import {
  ACTIVITY_KIND_LABELS, activityStatusLabel, directionLabel, formatDurationMs, formatTimestamp,
} from './workspaceModel';

const KIND_ICONS: Record<string, string> = { call: '☎', note: '✎', email: '✉', meeting: '▦' };

/** 本文の見せ方: HTML があれば無害化したもの、無ければ平文 (改行を保つ) */
export interface BodyView {
  /** 無害化済みの HTML 本文 (HTML が無ければ null) */
  html: string | null;
  /** HTML が無いときの平文 */
  text: string;
  /** 引用 (以前のやりとり)。無ければ空 */
  quotedHtml: string;
  quotedText: string;
  /** 折りたたみ時の下見 (最初の数行) */
  preview: string;
}

export function buildBodyView(a: WorkspaceActivity): BodyView {
  const rawHtml = a.rich?.body_html ?? null;
  if (rawHtml !== null && rawHtml.trim() !== '') {
    const safe = sanitizeHtml(rawHtml);
    const { main, quoted } = splitQuotedHtml(safe);
    return { html: main, text: '', quotedHtml: quoted, quotedText: '', preview: previewLines(htmlToText(main)) };
  }
  const full = a.rich?.body_full ?? a.body ?? '';
  const { main, quoted } = splitQuotedText(full);
  return { html: null, text: main, quotedHtml: '', quotedText: quoted, preview: previewLines(main) };
}

function addressLine(a: WorkspaceActivity): string | null {
  const r = a.rich;
  if (!r) return null;
  const from = [r.from_name, r.from_email ? (r.from_name ? `<${r.from_email}>` : r.from_email) : null].filter(Boolean).join(' ');
  const to = r.to.join(', ');
  if (!from && !to) return null;
  return `${from || '差出人不明'} → ${to || '宛先不明'}`;
}

function meetingTime(a: WorkspaceActivity): string | null {
  const s = formatTimestamp(a.rich?.start_time ?? null);
  if (!s) return null;
  const e = formatTimestamp(a.rich?.end_time ?? null);
  return e ? `${s} 〜 ${e.slice(11)}` : s;
}

function ActivityCardImpl({ a, ownerNames }: { a: WorkspaceActivity; ownerNames: ReadonlyMap<string, string> }) {
  const [open, setOpen] = useState(false);
  const [showQuote, setShowQuote] = useState(false);
  const view = useMemo(() => buildBodyView(a), [a]);
  const when = formatTimestamp(a.timestamp);
  const who = a.owner_id ? (ownerNames.get(a.owner_id) ?? null) : null;
  const dir = directionLabel(a.direction);
  const status = activityStatusLabel(a.status);
  const dur = formatDurationMs(a.duration_ms);
  const isEmail = a.kind === 'email';
  const addr = isEmail ? addressLine(a) : null;
  const cc = isEmail && a.rich && a.rich.cc.length > 0 ? `CC: ${a.rich.cc.join(', ')}` : null;
  const when2 = a.kind === 'meeting' ? meetingTime(a) : null;
  const attach = a.rich?.attachments_count ?? null;
  const metaParts = [
    isEmail ? null : dir, status, dur && `通話時間 ${dur}`, when2, a.rich?.location ? `場所: ${a.rich.location}` : null,
    attach !== null && `添付 ${String(attach)} 件`,
    a.via === 'contact' && '担当者の通話(別の案件のものを含む場合があります)',
  ].filter(Boolean);
  const hasBody = view.html !== null ? view.html.trim() !== '' : view.text.trim() !== '';
  const hasQuote = view.quotedHtml.trim() !== '' || view.quotedText.trim() !== '';
  const previewLineCount = view.preview === '' ? 0 : view.preview.split('\n').length;
  const fullLineCount = (view.html !== null ? htmlToText(view.html) : view.text).split('\n').filter(l => l.trim() !== '').length;
  const expandable = hasBody && (view.html !== null || fullLineCount > previewLineCount || view.preview.length < view.text.trim().length) || hasQuote;
  const recording = a.rich?.recording_url ?? null;
  const dirClass = dir === '受信' || dir === '着信' ? 'in' : 'out';

  return <li className={`wd-act wd-act-${a.kind} wd-card`} data-testid="activity-card" data-kind={a.kind}>
    <div className="wd-act-head">
      <span className="crm-status wd-card-badge"><span aria-hidden="true">{KIND_ICONS[a.kind] ?? '•'}</span> {ACTIVITY_KIND_LABELS[a.kind] ?? a.kind}</span>
      {isEmail && dir !== null && <span className={`wd-card-dir wd-card-dir-${dirClass}`} data-testid="activity-direction">{dir}</span>}
      {a.title && <strong className="wd-card-title" data-testid="activity-title">{a.title}</strong>}
      <small>{when ?? '日時不明'}</small>
      {who && <small className="wd-act-who">{who}</small>}
    </div>
    {addr && <p className="wd-card-addr" data-testid="activity-addr">{addr}{cc && <span className="wd-card-cc"> ({cc})</span>}</p>}
    {metaParts.length > 0 && <p className="wd-act-meta">{metaParts.join(' · ')}</p>}
    {recording && <p className="wd-act-meta"><a href={recording} target="_blank" rel="noopener noreferrer">録音を聞く</a></p>}
    {hasBody && !open && <p className="wd-card-preview" data-testid="activity-preview">{view.preview}</p>}
    {hasBody && open && (view.html !== null
      ? <div className="wd-card-body wd-card-html" data-testid="activity-body" dangerouslySetInnerHTML={{ __html: view.html }} />
      : <div className="wd-card-body wd-card-text" data-testid="activity-body">{view.text}</div>)}
    {open && hasQuote && <div className="wd-card-quote">
      <button type="button" className="wd-card-more" aria-expanded={showQuote} onClick={() => { setShowQuote(v => !v); }}>
        {showQuote ? '以前のやりとりを隠す' : '以前のやりとりを表示'}
      </button>
      {showQuote && (view.quotedHtml !== ''
        ? <div className="wd-card-body wd-card-html wd-card-quoted" data-testid="activity-quoted" dangerouslySetInnerHTML={{ __html: view.quotedHtml }} />
        : <div className="wd-card-body wd-card-text wd-card-quoted" data-testid="activity-quoted">{view.quotedText}</div>)}
    </div>}
    {expandable && <button type="button" className="wd-card-more" aria-expanded={open} onClick={() => { setOpen(v => !v); }}>
      {open ? '閉じる' : '全文を表示'}
    </button>}
  </li>;
}

export const ActivityCard = memo(ActivityCardImpl);
