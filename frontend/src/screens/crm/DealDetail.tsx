import { useState } from 'react';
import type { WorkspaceActivity } from '../../generated/WorkspaceActivity';
import type { WorkspaceResponse } from '../../generated/WorkspaceResponse';
import { dateValue } from './queueModel';
import { formatPhoneForDisplay, toDomesticPhone } from './phone';
import { toE164Jp } from './smartEmbed';
import type { DialResult, ZoomPhone } from './useZoomPhone';
import type { DetailState } from './useDealDetail';
import {
  ACTIVITY_FILTERS, ACTIVITY_KIND_LABELS, directionLabel, filterActivities, formatDurationMs, formatTimestamp, partialNotes,
} from './workspaceModel';
import type { ActivityKindFilter } from './workspaceModel';

/** HubSpot の選択肢の値 → 表示ラベル (不通時チェック bpo_10・ブロック理由 bpo_4)。定義がまだ無いときは値のまま */
export type StopLabel = (property: 'bpo_10' | 'bpo_4', value: string) => string;
export const rawStopLabel: StopLabel = (_p, v) => v;

const SOURCE_LABELS: Record<string, string> = { deal: '案件の番号', contact: '担当者の電話', mobile: '担当者の携帯', company: '会社の電話' };
const ymd = (raw: string | null) => dateValue(raw)?.replaceAll('-', '/') ?? null;

const DIAL_MESSAGES: Record<DialResult, string> = {
  sent: '発信を依頼しました。右の Zoom Phone を確認してください。',
  not_dialable: 'この番号はダイヤルできる形式ではありません。',
  embed_loading: 'Zoom Phone を読み込み中です。右の枠が表示されてから発信するか、番号のコピー・電話番号のリンク(tel:)を使ってください。',
  embed_unavailable: 'Zoom Phone が使えません。番号のコピーか、電話番号のリンク(tel:)を使ってください。',
  busy: '通話中のため、新しい発信はできません。',
};

/**
 * 電話番号 1 つ分: 表示(ハイフン区切り) / 発信(Smart Embed、元の値) / コピー(国内形式の数字) / tel:
 * `primary` は「架ける番号」の大きい表示
 */
export function PhoneRow({ label, raw, zoom, primary = false }: { label: string; raw: string; zoom: ZoomPhone; primary?: boolean }) {
  const shown = formatPhoneForDisplay(raw) ?? raw;
  const copyValue = toDomesticPhone(raw) ?? raw;
  const e164 = toE164Jp(raw);
  const [note, setNote] = useState('');
  function dial() { setNote(DIAL_MESSAGES[zoom.dial(raw)]); }
  function copy() {
    const clip = typeof navigator === 'undefined' ? undefined : (navigator as { clipboard?: Clipboard }).clipboard;
    if (!clip) { setNote('コピーできませんでした。番号を選んでコピーしてください。'); return; }
    clip.writeText(copyValue).then(() => { setNote('番号をコピーしました。'); }, () => { setNote('コピーできませんでした。番号を選んでコピーしてください。'); });
  }
  return <div className={`wd-phone${primary ? ' wd-phone-primary' : ''}`}>
    <span className="wd-phone-label">{label}</span>
    <span className="cq-phone" title={raw}>{shown}</span>
    <span className="wd-phone-actions">
      <button type="button" className="wd-dial" onClick={dial} disabled={e164 === null || zoom.embed === 'disabled'}
        aria-label={`${label} ${shown} に発信`}>発信</button>
      <button type="button" className="wd-copy" onClick={copy} aria-label={`${label} ${shown} の番号をコピー`}>番号をコピー</button>
      {e164 !== null && <a href={`tel:${e164}`} aria-label={`${label} ${shown} へ tel: で発信`}>tel:</a>}
    </span>
    {e164 === null && <small className="crm-muted">ダイヤルできる形式ではありません</small>}
    {note && <small role="status">{note}</small>}
  </div>;
}

function ActivityItem({ a }: { a: WorkspaceActivity }) {
  const when = formatTimestamp(a.timestamp);
  const dir = directionLabel(a.direction);
  const dur = formatDurationMs(a.duration_ms);
  return <li className={`wd-act wd-act-${a.kind}`}>
    <div className="wd-act-head">
      <span className="crm-status">{ACTIVITY_KIND_LABELS[a.kind] ?? a.kind}</span>
      {a.title && <strong>{a.title}</strong>}
      <small>{when ?? '日時不明'}</small>
    </div>
    {(dir !== null || a.status !== null || dur !== null || a.via === 'contact') && <p className="wd-act-meta">
      {[dir, a.status, dur && `通話時間 ${dur}`, a.via === 'contact' && '担当者の通話(別の案件のものを含む場合があります)'].filter(Boolean).join(' · ')}
    </p>}
    {a.body && <p className="wd-act-body">{a.body}</p>}
  </li>;
}

function Detail({ data, zoom, ownerName, stopLabel }: { data: WorkspaceResponse; zoom: ZoomPhone; ownerName?: string | undefined; stopLabel: StopLabel }) {
  const [kind, setKind] = useState<ActivityKindFilter>('all');
  const d = data.deal;
  const company = data.companies.find(c => c.is_primary) ?? null;
  const acts = filterActivities(data.activities, kind);
  const notes = partialNotes(data.partial);
  const next = ymd(d.next_call_date);
  const last = ymd(d.last_call_date);
  const stopReasons = [d.stop.prohibited_reason && `架電禁止理由: ${d.stop.prohibited_reason}`, d.stop.block_reason && `ブロック理由: ${stopLabel('bpo_4', d.stop.block_reason)}`,
    d.stop.unreachable_check && `不通時チェック: ${stopLabel('bpo_10', d.stop.unreachable_check)}`].filter((x): x is string => typeof x === 'string');
  const otherPhones = [
    ...data.contacts.flatMap(c => [c.phone && { key: `${c.id}-p`, label: `${c.name ?? '担当者'}の電話`, raw: c.phone },
      c.mobile && { key: `${c.id}-m`, label: `${c.name ?? '担当者'}の携帯`, raw: c.mobile }]),
    ...data.companies.map(c => c.phone && { key: `${c.id}-c`, label: `${c.name ?? '会社'}の電話`, raw: c.phone }),
  ].filter((x): x is { key: string; label: string; raw: string } => typeof x === 'object' && x !== null && x.raw !== data.dial?.number);

  return <article className="wd" aria-label="架電先の詳細">
    {/* 上端に固定: 会社・案件・ステージ・HubSpot と「架ける番号」。下の情報だけがスクロールする */}
    <div className="wd-top">
      <header className="wd-head">
        <div className="wd-head-main">
          <h2>{company?.name ?? d.name ?? '(名称なし)'}</h2>
          <p>{d.name ?? '(案件名なし)'}</p></div>
        <div className="wd-head-side">
          <span className="cq-stage">{d.stage_label ?? '(ステージ名を取得できません)'}</span>
          <a href={d.deep_link} target="_blank" rel="noreferrer">HubSpotで開く</a>
        </div>
      </header>

      <section className="wd-dialbox" aria-label="架ける番号">
        <h3 className="wd-dial-title">架ける番号</h3>
        {data.dial ? <PhoneRow label={SOURCE_LABELS[data.dial.source] ?? '電話'} raw={data.dial.number} zoom={zoom} primary />
          : <p className="crm-muted">番号を確認できません。担当者・会社の情報を HubSpot で確認してください。</p>}
        {otherPhones.length > 0 && <details className="wd-other"><summary>ほかの番号({otherPhones.length})</summary>
          {otherPhones.map(p => <PhoneRow key={p.key} label={p.label} raw={p.raw} zoom={zoom} />)}</details>}
      </section>
    </div>

    <div className="wd-body">
    {notes.length > 0 && <div className="cq-notice cq-warn" role="status"><strong>一部の情報が欠けています</strong>
      <ul>{notes.map(n => <li key={n}>{n}</li>)}</ul></div>}

    <section className="wd-card" aria-label="案件の情報">
      <h3>案件</h3>
      <dl className="wd-dl">
        <div><dt>担当</dt><dd>{d.owner_id ? (ownerName ?? '担当あり') : '担当なし'}</dd></div>
        <div><dt>次回架電</dt><dd>{next ? <>{next}{d.next_call_time && ` ${d.next_call_time}`}</> : <span className="crm-muted">なし</span>}</dd></div>
        <div><dt>最終架電日</dt><dd>{last ?? <span className="crm-muted">未架電</span>}</dd></div>
        <div><dt>金額</dt><dd>{d.amount ? `${Number(d.amount).toLocaleString('ja-JP')} 円` : <span className="crm-muted">未設定</span>}</dd></div>
      </dl>
      {stopReasons.length > 0 && <ul className="wd-stop">{stopReasons.map(s => <li key={s} className="cq-flag">{s}</li>)}</ul>}
    </section>

    <section className="wd-card" aria-label="担当者">
      <h3>担当者{data.contacts_total > data.contacts.length && <small>(紐づく {data.contacts_total} 人のうち {data.contacts.length} 人を表示)</small>}</h3>
      {data.contacts.length === 0 && <p className="crm-muted">担当者の情報を取得できませんでした(HubSpot に紐づく担当者がいない、または取得に失敗)。</p>}
      <ul className="wd-list">{data.contacts.map(c => <li key={c.id}>
        <strong>{c.name ?? '(氏名なし)'}</strong>{c.is_primary && <span className="crm-status">主</span>}
        {c.job_title && <small>{c.job_title}</small>}
        {c.email && <small>{c.email}</small>}
        <small>{[c.phone && `電話 ${formatPhoneForDisplay(c.phone) ?? c.phone}`, c.mobile && `携帯 ${formatPhoneForDisplay(c.mobile) ?? c.mobile}`].filter(Boolean).join(' / ') || '電話番号の登録なし'}</small>
        <a href={c.deep_link} target="_blank" rel="noreferrer">HubSpotで開く</a>
      </li>)}</ul>
    </section>

    <section className="wd-card" aria-label="会社">
      <h3>会社</h3>
      {!company && <p className="crm-muted">会社の情報を取得できませんでした。</p>}
      {company && <dl className="wd-dl">
        <div><dt>会社名</dt><dd>{company.name ?? '(名称なし)'}</dd></div>
        <div><dt>電話</dt><dd>{company.phone ? (formatPhoneForDisplay(company.phone) ?? company.phone) : <span className="crm-muted">なし</span>}</dd></div>
        <div><dt>住所</dt><dd>{company.address ?? <span className="crm-muted">なし</span>}</dd></div>
        <div><dt>業種</dt><dd>{company.industry ?? <span className="crm-muted">なし</span>}</dd></div>
        <div><dt>サイト</dt><dd>{company.domain ?? <span className="crm-muted">なし</span>}</dd></div>
      </dl>}
      {company && <a href={company.deep_link} target="_blank" rel="noreferrer">HubSpotで開く</a>}
      {data.companies_total > 1 && <small>紐づく会社は {data.companies_total} 社です。</small>}
    </section>

    <section className="wd-card" aria-label="活動履歴">
      <h3>活動履歴<small>({data.activities.length} 件{data.activities_truncated ? '・これより古い履歴は表示していません' : ''})</small></h3>
      <p className="wd-scope">{data.activity_scope}</p>
      <div className="wd-filters" role="group" aria-label="活動の種類">
        {ACTIVITY_FILTERS.map(f => <button key={f.value} type="button" aria-pressed={kind === f.value} onClick={() => { setKind(f.value); }}>{f.label}</button>)}
      </div>
      {acts.length === 0 && <p className="crm-muted">{data.activities.length === 0 ? '表示できる活動履歴はありません。' : 'この種類の活動はありません。'}</p>}
      <ul className="wd-acts">{acts.map(a => <ActivityItem key={`${a.kind}-${a.id}`} a={a} />)}</ul>
    </section>

    </div>
  </article>;
}

function DetailMessage({ state, reload }: { state: DetailState; reload: () => void }) {
  if (state.phase === 'idle') return <div className="cq-notice cq-empty wd-empty"><strong>左の一覧から架電先を選んでください</strong>
    <p>案件・担当者・会社・活動履歴を HubSpot から読み込みます。</p></div>;
  if (state.phase === 'loading') return <p role="status" className="cq-loading">詳細を読み込み中…</p>;
  if (state.phase === 'forbidden') return <div className="cq-notice cq-error" role="alert"><strong>表示できません</strong><p>{state.message}</p></div>;
  return <div className="cq-notice cq-error" role="alert"><strong>詳細を取得できませんでした</strong>
    <p>{state.message || '取得に失敗しました。'}</p><button type="button" onClick={reload}>再試行</button></div>;
}

export function DealDetail({ state, reload, zoom, ownerName, stopLabel = rawStopLabel }: {
  state: DetailState; reload: () => void; zoom: ZoomPhone; ownerName?: string | undefined; stopLabel?: StopLabel;
}) {
  if (state.phase === 'ready' && state.data !== null) return <Detail data={state.data} zoom={zoom} ownerName={ownerName} stopLabel={stopLabel} />;
  return <div className="cq-detail-scroll"><DetailMessage state={state} reload={reload} /></div>;
}
