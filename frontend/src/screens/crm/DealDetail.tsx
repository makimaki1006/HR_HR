import { EmailPanel } from "./EmailPanel";
import { EmailPendingLog } from "./EmailPendingLog";
import { ReplySyncStatus } from "./ReplySyncStatus";
import { memo, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { EMAIL_SAVED } from './emailApi';
import type { SavedEmailNotice } from './emailApi';
import type { WorkspaceResponse } from '../../generated/WorkspaceResponse';
import type { CallQueueItem } from '../../generated/CallQueueItem';
import { RowPreview } from './RowPreview';
import { formatPhoneForDisplay, toDomesticPhone } from './phone';
import { toE164Jp } from './smartEmbed';
import type { DialResult, ZoomPhone } from './useZoomPhone';
import type { DetailState } from './useDealDetail';
import {
  ACTIVITY_FILTERS, ACTIVITY_KIND_LABELS, filterActivities, formatTimestamp, partialNotes,
} from './workspaceModel';
import type { ActivityKindFilter } from './workspaceModel';
import { FRESHNESS_TICK_MS, clock, freshnessLabel } from './workspaceModel';
import { ActivityCard } from './ActivityCard';
import { EmailThreadCard } from './EmailThreadCard';
import { groupActivityEmails } from './emailThreads';
import { RESULT_LABELS, useTicking } from './ZoomPhonePanel';
import type { ZoomEvent } from './smartEmbed';
import { StageMover } from './StageMove';
import type { StageMoveCtx } from './writeBindings';

/** HubSpot の選択肢の値 → 表示ラベル (不通時チェック bpo_10・ブロック理由 bpo_4)。定義がまだ無いときは値のまま */
export type StopLabel = (property: 'bpo_10' | 'bpo_4', value: string) => string;
export const rawStopLabel: StopLabel = (_p, v) => v;

const SOURCE_LABELS: Record<string, string> = { deal: '案件の番号', contact: '担当者の電話', mobile: '担当者の携帯', company: '会社の電話' };

// 発信を依頼できたときの様子は「架ける番号」の下の 1 行 (CallBar) に出すので、ここでは何も言わない
const DIAL_MESSAGES: Record<DialResult, string> = {
  sent: '',
  not_dialable: 'この番号はダイヤルできる形式ではありません。',
  embed_loading: 'Zoom を読み込み中です。少し待ってからもう一度発信するか、「番号をコピー」か「端末の電話で発信」を使ってください。',
  embed_unavailable: 'Zoom Phone が使えません。「番号をコピー」か「端末の電話で発信」を使ってください。',
  busy: '通話中のため、新しい発信はできません。',
  // ログインの案内は「架ける番号」の欄の警告 (ZoomWarn) に出す
  login_required: '',
};

/**
 * 電話番号 1 つ分: 表示(ハイフン区切り) / 発信(Smart Embed、元の値) / コピー(国内形式の数字) / tel:
 * `primary` は「架ける番号」の大きい表示
 */
/** 発信を押せない理由 (押せるときは null)。判定は PhoneRow の disabled と同じ */
export function dialDisabledReason(e164: string | null, embed: ZoomPhone['embed'], blocked: boolean, blockReason?: string | null): string | null {
  if (blocked) return blockReason ?? '架電禁止またはブロックのため発信できません';
  if (e164 === null) return 'ダイヤルできる形式ではないため発信できません';
  if (embed === 'disabled') return 'Zoom を読み込めないため発信できません';
  return null;
}

export function PhoneRow({ label, raw, zoom, primary = false, compact = false, blocked = false, blockReason = null }: {
  label: string; raw: string; zoom: ZoomPhone; primary?: boolean;
  /** 架電禁止・ブロックの理由の文言 (発信を押せない理由として出す) */
  blockReason?: string | null;
  /** 1 行の表示 (番号と発信だけ。コピーと端末の電話は「詳しく表示」で出す) */
  compact?: boolean;
  blocked?: boolean;
}) {
  const shown = formatPhoneForDisplay(raw) ?? raw;
  const copyValue = toDomesticPhone(raw) ?? raw;
  const e164 = toE164Jp(raw);
  const [note, setNote] = useState('');
  const disabledReason = dialDisabledReason(e164, zoom.embed, blocked, blockReason);
  function dial() { setNote(DIAL_MESSAGES[zoom.dial(raw)]); }
  function copy() {
    const clip = typeof navigator === 'undefined' ? undefined : (navigator as { clipboard?: Clipboard }).clipboard;
    if (!clip) { setNote('コピーできませんでした。番号を選んでコピーしてください。'); return; }
    clip.writeText(copyValue).then(() => { setNote('番号をコピーしました。'); }, () => { setNote('コピーできませんでした。番号を選んでコピーしてください。'); });
  }
  if (compact) {
    return <span className="wd-phone wd-phone-primary wd-phone-compact">
      <span className="cq-phone" title={`${label}: ${raw}`}>{shown}</span>
      <button type="button" className="wd-dial" onClick={dial} disabled={disabledReason !== null} title={disabledReason ?? undefined}
        aria-label={`${label} ${shown} に発信`}>発信</button>
      {disabledReason !== null && <small className="wd-dial-reason" data-testid="dial-disabled-reason">{disabledReason}</small>}
      {note && <small role="status">{note}</small>}
    </span>;
  }
  return <div className={`wd-phone${primary ? ' wd-phone-primary' : ''}`}>
    <span className="wd-phone-label">{label}</span>
    <span className="cq-phone" title={raw}>{shown}</span>
    <span className="wd-phone-actions">
      <button type="button" className="wd-dial" onClick={dial} disabled={disabledReason !== null} title={disabledReason ?? undefined}
        aria-label={`${label} ${shown} に発信`}>発信</button>
      <button type="button" className="wd-copy" onClick={copy} aria-label={`${label} ${shown} の番号をコピー`}>番号をコピー</button>
      {!blocked && e164 !== null && <a href={`tel:${e164}`} aria-label={`${label} ${shown} へ端末の電話で発信`}
        title="このパソコン・スマートフォンの電話アプリで発信します(tel: リンク)">端末の電話で発信</a>}
    </span>
    {disabledReason !== null && <small className="wd-dial-reason" data-testid="dial-disabled-reason">{disabledReason}</small>}
    {note && <small role="status">{note}</small>}
  </div>;
}

/** 「架ける番号」の下に出す通話の様子 (Zoom の枠を閉じていても分かるように) */
export type CallBarInfo =
  | { kind: 'dialing'; number: string | null }
  | { kind: 'failed' }
  | { kind: 'ringing'; number: string | null; inbound: boolean }
  | { kind: 'connected'; number: string | null; connectedAt: number | null }
  | { kind: 'ended'; talkSeconds: number | null; result: ZoomEvent['result'] };

/** Zoom が発信の依頼に応えなかったときの案内 */
export const ZOOM_NO_RESPONSE = 'Zoomが応答しません。Zoomアプリを起動してサインインしてから、もう一度発信してください';

export const ZOOM_LOGIN_REQUIRED = 'Zoom にログインしてください。ログインできたら、もう一度発信を押してください';
export const ZOOM_LOGIN_DONE = 'Zoom にログインしました。発信ボタンを押してください';

/** 発信の近くに出す Zoom の警告 (使えるときは null = 何も出さない)。架空サンプルでは出さない */
export function zoomWarning(zoom: ZoomPhone, sample: boolean, callFailed: boolean): string | null {
  if (sample || zoom.call.phase === 'ringing' || zoom.call.phase === 'connected') return null;
  if (zoom.loginNotice === 'required' && zoom.embed !== 'timeout') return ZOOM_LOGIN_REQUIRED;
  if (zoom.loginNotice === 'done') return ZOOM_LOGIN_DONE;
  if (zoom.embed === 'timeout') return 'Zoom Phone が使えません。Zoomアプリを起動してサインインしてください';
  if (zoom.embed === 'loading') return 'Zoom Phone を準備しています。少しお待ちください';
  if (zoom.stalled && !callFailed) return ZOOM_NO_RESPONSE;
  return null;
}

/** 通話の様子の文言 (と、終わった通話の通話時間) */
export function callBarLabel(info: CallBarInfo): { label: string; extra: string | null } {
  if (info.kind === 'dialing') return { label: '発信しています…', extra: null };
  if (info.kind === 'failed') return { label: '発信できませんでした', extra: null };
  if (info.kind === 'ringing') return { label: info.inbound ? '着信中' : '呼び出し中', extra: null };
  if (info.kind === 'connected') return { label: '通話中', extra: null };
  return {
    label: (info.result ? RESULT_LABELS[info.result] : undefined) ?? '通話が終了しました',
    extra: info.talkSeconds !== null ? `(通話時間 ${clock(info.talkSeconds)})` : null,
  };
}

export function CallBar({ info, now, onOpenZoom }: { info: CallBarInfo; now: () => number; onOpenZoom?: (() => void) | undefined }) {
  const connectedAt = info.kind === 'connected' ? info.connectedAt : null;
  const t = useTicking(connectedAt !== null, now);
  const who = 'number' in info ? formatPhoneForDisplay(info.number) : null;
  const { label, extra } = callBarLabel(info);
  return <div className={`wd-callbar wd-callbar-${info.kind}`} data-testid="call-bar">
    <p className="wd-callbar-line">
      <span role="status" className="wd-callbar-status" data-testid="call-bar-status"><strong>{label}</strong>{extra && <span> {extra}</span>}</span>
      {/* 毎秒変わる時計は読み上げない (状態の変化だけを読み上げる) */}
      {connectedAt !== null && <span className="wd-callbar-timer" aria-hidden="true" data-testid="call-bar-timer">{clock((t - connectedAt) / 1000)}</span>}
      {who && <span className="wd-callbar-who">{who}</span>}
      {onOpenZoom && <button type="button" className="wd-callbar-open" onClick={onOpenZoom}
        title="消音・保留・通話を切る・数字の入力は Zoom の枠で行います">Zoomを開く</button>}
    </p>
    {info.kind === 'failed' && <p className="wd-callbar-msg" role="alert">{ZOOM_NO_RESPONSE}</p>}
  </div>;
}

/**
 * 詳細を読んだ時刻からの経過 (「○秒前の情報」、15 秒ごとに描き直す) と「最新にする」。
 * サーバは同じ案件の詳細を 60 秒まで使い回すので、HubSpot で変えた直後はここで読み直す
 */
export function Freshness({ fetchedAt, now, refreshing, refreshError, onRefresh }: {
  fetchedAt: string; now: () => number; refreshing: boolean; refreshError: string | null; onRefresh?: (() => void) | undefined;
}) {
  const t = useTicking(true, now, FRESHNESS_TICK_MS);
  const label = freshnessLabel(fetchedAt, t);
  return <p className="wd-fresh" data-testid="detail-freshness">
    {label !== null && <span className="wd-fresh-age" title={`HubSpot から読んだ時刻: ${new Date(fetchedAt).toLocaleString('ja-JP')}`}
      data-testid="detail-freshness-age">{refreshing ? '最新の情報を読み込み中…' : label}</span>}
    {onRefresh && <button type="button" className="wd-fresh-btn" onClick={onRefresh} disabled={refreshing}
      title="HubSpot から読み直します">最新にする</button>}
    {refreshError !== null && !refreshing && <span className="wd-fresh-error" role="alert">最新の情報を読めませんでした。表示は前の内容のままです({refreshError})</span>}
  </p>;
}

/** 「案件の概要」の表示の切り替え (1 行 ⇔ 詳しく) */
export interface OverviewDensity {
  compact: boolean;
  onToggle: () => void;
}

function DensityToggle({ density }: { density: OverviewDensity }) {
  return <button type="button" className="wd-density" aria-expanded={!density.compact} onClick={density.onToggle}
    title={density.compact ? '会社・案件・番号の詳細、ほかの番号、架電の注意を表示します' : '会社・担当者・番号・発信だけの 1 行にします'}>
    {density.compact ? '詳しく表示' : '1 行にする'}</button>;
}

/** 前回の活動 1 行 (活動の先頭 1 件)。活動が無ければ null */
export function lastActivityLine(activities: WorkspaceResponse['activities']): { text: string; full: string } | null {
  const a = activities[0];
  if (!a) return null;
  const when = formatTimestamp(a.timestamp)?.match(/\/(\d{1,2})\/(\d{1,2})/u);
  const date = when ? `${when[1]?.padStart(2, '0') ?? ''}/${when[2]?.padStart(2, '0') ?? ''}` : null;
  const kind = ACTIVITY_KIND_LABELS[a.kind] ?? a.kind;
  const body = (a.body ?? a.title ?? '').replace(/\s+/gu, ' ').trim();
  const full = `前回 ${date ? `${date} ` : ''}${kind}${body ? `・${body}` : ''}`;
  return { text: full, full };
}

function LastActivity({ data, onOpenActivity }: { data: WorkspaceResponse; onOpenActivity?: (() => void) | undefined }) {
  const line = lastActivityLine(data.activities);
  if (line === null) return null;
  // 押すと「活動ログ」のタブを前に出す (つながっていないときは文字だけ)
  if (onOpenActivity === undefined) return <p className="wd-last" title={line.full} data-testid="last-activity">{line.text}</p>;
  return <p className="wd-last" data-testid="last-activity">
    <button type="button" className="wd-last-btn" title={`${line.full}（押すと活動ログを開きます）`} onClick={onOpenActivity}>{line.text}</button></p>;
}

function ZoomWarn({ text, onOpenZoom }: { text: string | null; onOpenZoom?: (() => void) | undefined }) {
  if (text === null) return null;
  return <p className="wd-zoom-warn" role="status" data-testid="zoom-warning"><span>{text}</span>
    {onOpenZoom && <button type="button" onClick={onOpenZoom}>Zoomを開く</button>}</p>;
}

function Overview({ data, zoom, stopLabel, callBar, onOpenZoom, refreshing, refreshError, onRefresh, density, stageMove, zoomNotice = null, onOpenActivity }: {
  zoomNotice?: string | null;
  onOpenActivity?: (() => void) | undefined;
  data: WorkspaceResponse; zoom: ZoomPhone; stopLabel: StopLabel;
  callBar?: CallBarInfo | null | undefined; onOpenZoom?: (() => void) | undefined;
  refreshing: boolean; refreshError: string | null; onRefresh?: (() => void) | undefined;
  density?: OverviewDensity | undefined;
  stageMove?: StageMoveCtx | null | undefined;
}) {
  const d = data.deal;
  const blocked = (Boolean(d.stop.prohibited_reason) || Boolean(d.stop.block_reason));
  const blockReason = d.stop.prohibited_reason ? '架電禁止のため発信できません' : d.stop.block_reason ? 'ブロックのため発信できません' : null;
  const company = data.companies.find(c => c.is_primary) ?? null;
  const notes = partialNotes(data.partial);
  const stopReasons = [d.stop.prohibited_reason && `架電禁止理由: ${d.stop.prohibited_reason}`, d.stop.block_reason && `ブロック理由: ${stopLabel('bpo_4', d.stop.block_reason)}`,
    d.stop.unreachable_check && `不通時チェック: ${stopLabel('bpo_10', d.stop.unreachable_check)}`].filter((x): x is string => typeof x === 'string');
  const otherPhones = [
    ...data.contacts.flatMap(c => [c.phone && { key: `${c.id}-p`, label: `${c.name ?? '担当者'}の電話`, raw: c.phone },
      c.mobile && { key: `${c.id}-m`, label: `${c.name ?? '担当者'}の携帯`, raw: c.mobile }]),
    ...data.companies.map(c => c.phone && { key: `${c.id}-c`, label: `${c.name ?? '会社'}の電話`, raw: c.phone }),
  ].filter((x): x is { key: string; label: string; raw: string } => typeof x === 'object' && x !== null && x.raw !== data.dial?.number);

  if (density?.compact === true) {
    // 1 行: 会社 · 担当者 · 架ける番号 · 発信 · 通話の様子 (列が低いとき。「詳しく表示」で戻す)
    const contact = data.contacts.find(c => c.is_primary) ?? data.contacts[0] ?? null;
    const status = callBar ? callBarLabel(callBar) : null;
    return <article className="wd wd-compact" aria-label="架電先の詳細" data-testid="overview-compact">
      <div className="wd-top wd-top-compact">
        <div className="wd-line wd-line-1" data-testid="head-line-1">
          <h2 className="wd-line-company" title={company?.name ?? d.name ?? undefined}>{company?.name ?? d.name ?? '(名称なし)'}</h2>
          <span className="wd-line-sep" aria-hidden="true">·</span>
          <span className="wd-line-contact" title={contact?.name ?? undefined}>{contact?.name ?? <span className="crm-muted">担当者なし</span>}</span>
          <span className="wd-line-end">
            <a href={d.deep_link} target="_blank" rel="noreferrer">HubSpotで開く</a>
            <DensityToggle density={density} />
          </span>
        </div>
        <div className="wd-line wd-line-2" data-testid="head-line-2">
          {data.dial ? <PhoneRow blocked={blocked} blockReason={blockReason} label={SOURCE_LABELS[data.dial.source] ?? '電話'} raw={data.dial.number} zoom={zoom} primary compact />
            : <span className="wd-dial-reason" data-testid="dial-disabled-reason">電話番号がありません</span>}
          {zoomNotice !== null && <ZoomWarn text={zoomNotice} onOpenZoom={onOpenZoom} />}
          {status !== null && callBar?.kind !== 'failed' && <span className={`wd-line-status wd-callbar-${callBar?.kind ?? ''}`} role="status" data-testid="call-bar-status">
            <strong>{status.label}</strong>{status.extra && <span> {status.extra}</span>}</span>}
          {status !== null && callBar?.kind !== 'failed' && onOpenZoom && <button type="button" className="wd-callbar-open" onClick={onOpenZoom}
            title="消音・保留・通話を切る・数字の入力は Zoom の枠で行います">Zoomを開く</button>}
          {stopReasons.length > 0 && <span className="cq-flag wd-line-flag" title={stopReasons.join(' / ')}>架電の注意 {String(stopReasons.length)} 件</span>}
          {notes.length > 0 && <span className="cq-flag wd-line-flag" title={notes.join(' / ')}>一部の情報が欠けています</span>}
        </div>
        <LastActivity data={data} onOpenActivity={onOpenActivity} />
        {/* 発信できなかったときは、何をすればよいかを 1 行の下に出す */}
        {callBar?.kind === 'failed' && <CallBar info={callBar} now={zoom.now} onOpenZoom={onOpenZoom} />}
      </div>
    </article>;
  }
  return <article className="wd" aria-label="架電先の詳細">
    {/* 会社・案件・ステージ・HubSpot と「架ける番号」・通話の様子 (置いた列の上端に固定) */}
    <div className="wd-top">
      {/* 概要の見出し: 並べ方は「このパネルの幅」で変わる (queue.css の @container wdsum。広い=3 列 / 中=2 列 / 狭い=縦に区切って積む) */}
      <header className="wd-summary" aria-label="案件の概要">
        <div className="wd-s-name">
          <h2 title={company?.name ?? d.name ?? undefined}>{company?.name ?? d.name ?? '(名称なし)'}</h2>
          <p title={d.name ?? undefined}>{d.name ?? '(案件名なし)'}</p></div>
        <div className="wd-s-stage">
          <StageMover ctx={stageMove} fallback={<span className="cq-stage">{d.stage_label ?? '(ステージ名を取得できません)'}</span>} />
        </div>
        <div className="wd-s-foot">
          <span className="wd-s-actions">
            <a href={d.deep_link} target="_blank" rel="noreferrer">HubSpotで開く</a>
            {density && <DensityToggle density={density} />}
          </span>
          <Freshness fetchedAt={data.fetched_at} now={zoom.now} refreshing={refreshing} refreshError={refreshError} onRefresh={onRefresh} />
        </div>
      </header>
      <LastActivity data={data} onOpenActivity={onOpenActivity} />

      <section className="wd-dialbox" aria-label="架ける番号">
        <h3 className="wd-dial-title">架ける番号</h3>
        {data.dial ? <PhoneRow blocked={blocked} blockReason={blockReason} label={SOURCE_LABELS[data.dial.source] ?? '電話'} raw={data.dial.number} zoom={zoom} primary />
          : <p className="crm-muted">番号を確認できません。担当者・会社の情報を HubSpot で確認してください。</p>}
        {otherPhones.length > 0 && <details className="wd-other"><summary>ほかの番号({otherPhones.length})</summary>
          <div className="wd-other-list">{otherPhones.map(p => <PhoneRow blocked={blocked} blockReason={blockReason} key={p.key} label={p.label} raw={p.raw} zoom={zoom} />)}</div></details>}
        <ZoomWarn text={zoomNotice} onOpenZoom={onOpenZoom} />
      </section>
      {callBar && <CallBar info={callBar} now={zoom.now} onOpenZoom={onOpenZoom} />}
      {stopReasons.length > 0 && <ul className="wd-stop" aria-label="架電の注意">{stopReasons.map(s => <li key={s} className="cq-flag">{s}</li>)}</ul>}
      {notes.length > 0 && <div className="cq-notice cq-warn wd-notes" role="status"><strong>一部の情報が欠けています</strong>
        <ul>{notes.map(n => <li key={n}>{n}</li>)}</ul></div>}
    </div>
  </article>;
}

/** 活動ログ (いつ・誰が・どうだったか)。種類で絞り込める */
function ActivityLogImpl({ data, placeholder, ownerNames, sample = false }: {
  data: WorkspaceResponse | null; placeholder: string; ownerNames: ReadonlyMap<string, string>; sample?: boolean;
}) {
  const [kind, setKind] = useState<ActivityKindFilter>('all');
  const log = useRef<HTMLElement>(null);
  const [notice, setNotice] = useState<SavedEmailNotice | null>(null);
  useEffect(() => {
    const saved = (event: Event) => {
      const value = (event as CustomEvent<SavedEmailNotice>).detail;
      if (value.dealId !== data?.deal.id) return;
      setKind('all'); setNotice(value);
      if (log.current) log.current.scrollTop = 0;
    };
    window.addEventListener(EMAIL_SAVED, saved);
    return () => { window.removeEventListener(EMAIL_SAVED, saved); };
  }, [data?.deal.id]);
  const visibleId = notice?.dealId === data?.deal.id ? notice?.emailId : null;
  const shown = visibleId !== null && data?.activities.some(a => a.kind === 'email' && a.id === visibleId) === true;
  useLayoutEffect(() => {
    if (shown && log.current) log.current.scrollTop = 0;
  }, [shown, visibleId]);
  useEffect(() => {
    if (!notice) return;
    // Start the five-second highlight after the refreshed record actually appears.
    const timer = setTimeout(() => { setNotice(null); }, shown ? 5000 : 60_000);
    return () => { clearTimeout(timer); };
  }, [notice, shown]);
  if (data === null) return <div className="dock-scroll"><p className="dock-placeholder">{placeholder}</p></div>;
  const acts = filterActivities(data.activities, kind);
  const entries = groupActivityEmails(acts);
  const emailTotal = acts.filter(a => a.kind === 'email').length;
  return <section ref={log} className="wd-log dock-scroll" aria-label="活動ログ" data-testid="activity-log">
    <p className="wd-log-head"><strong>{entries.length} 件</strong><span className="cq-info wd-scope" data-glyph="ⓘ" tabIndex={0} role="img" aria-label={data.activity_scope} title={data.activity_scope} data-testid="activity-scope" />{emailTotal > 0 && <small>（メール {emailTotal} 件）</small>}{data.activities_truncated && <small>(これより古い履歴は表示していません)</small>}</p>
        <ReplySyncStatus key={`reply:${data.deal.id}`} dealId={data.deal.id} sample={sample} />
    <div className="wd-filters" role="group" aria-label="活動の種類">
      {ACTIVITY_FILTERS.map(f => <button key={f.value} type="button" aria-pressed={kind === f.value} onClick={() => { setKind(f.value); }}>{f.label}</button>)}
    </div>
    {acts.length === 0 && <p className="crm-muted">{data.activities.length === 0 ? '表示できる活動履歴はありません。' : 'この種類の活動はありません。'}</p>}
    {(kind === 'all' || kind === 'email') && <EmailPendingLog key={`pending:${data.deal.id}`} dealId={data.deal.id} sample={sample} />}
    <ul className="wd-acts">{entries.map(entry => {
      if (entry.kind === 'thread' && entry.messages.length > 1) return <EmailThreadCard key={entry.key} thread={entry} ownerNames={ownerNames} highlightedId={shown ? visibleId ?? null : null} />;
      const a = entry.kind === 'thread' ? entry.messages[0] : entry.activity;
      return a ? <ActivityCard key={entry.key} a={a} ownerNames={ownerNames} highlighted={shown && a.kind === 'email' && a.id === visibleId} /> : null;
    })}</ul>
  </section>;
}

/** 親 (架電画面) が描き直しても、props が同じなら描き直さない */
export const ActivityLog = memo(ActivityLogImpl);

function DetailMessage({ state, reload }: { state: DetailState; reload: () => void }) {
  if (state.phase === 'idle') return <div className="cq-notice cq-empty wd-empty"><strong>取引一覧から架電先を選んでください</strong>
    <p>案件・担当者・会社・活動履歴を HubSpot から読み込みます。</p></div>;
  if (state.phase === 'loading') return <p role="status" className="cq-loading">詳細を読み込み中…</p>;
  if (state.phase === 'forbidden') return <div className="cq-notice cq-error" role="alert"><strong>表示できません</strong><p>{state.message}</p></div>;
  return <div className="cq-notice cq-error" role="alert"><strong>詳細を取得できませんでした</strong>
    <p>{state.message || '取得に失敗しました。'}</p><button type="button" onClick={reload}>再試行</button></div>;
}

function DealOverviewImpl({ state, reload, refresh, zoom, stopLabel = rawStopLabel, callBar, onOpenZoom, density, stageMove, sample = false, previewItem, previewOwner, onOpenActivity }: {
  sample?: boolean;
  /** 「前回 …」の行を押したときに活動ログを前に出す */
  onOpenActivity?: (() => void) | undefined;
  /** 詳細を読み込む間、見出しに出す一覧の行 (無ければ「読み込み中」だけ) */
  previewItem?: CallQueueItem | null | undefined;
  previewOwner?: string | undefined;
  state: DetailState; reload: () => void; zoom: ZoomPhone; stopLabel?: StopLabel;
  /** サーバのキャッシュを使わずに読み直す (「最新にする」) */
  refresh?: (() => void) | undefined;
  /** 「架ける番号」の下に出す通話の様子 (出すものが無ければ null) */
  callBar?: CallBarInfo | null | undefined;
  /** Zoom の枠を開く */
  onOpenZoom?: (() => void) | undefined;
  /** 1 行の表示にするか (と切り替え)。無ければ常に詳しく */
  density?: OverviewDensity | undefined;
  /** ステージの変更 (無ければ表示だけ) */
  stageMove?: StageMoveCtx | null | undefined;
}) {
  if (state.phase === 'ready' && state.data !== null) {
    return <><Overview data={state.data} zoom={zoom} stopLabel={stopLabel} callBar={callBar} onOpenZoom={onOpenZoom}
      zoomNotice={zoomWarning(zoom, sample, callBar?.kind === 'failed')}
      refreshing={state.refreshing} refreshError={state.refreshError} onRefresh={refresh} density={density} stageMove={stageMove} onOpenActivity={onOpenActivity} /><EmailPanel key={state.data.deal.id} data={state.data} sample={sample} onSaved={refresh} context={<div className="crm-email-context"><strong>{state.data.companies.find(c => c.is_primary)?.name ?? state.data.deal.name}</strong>{state.data.dial && <PhoneRow blocked={(Boolean(state.data.deal.stop.prohibited_reason) || Boolean(state.data.deal.stop.block_reason))} label={SOURCE_LABELS[state.data.dial.source] ?? '電話'} raw={state.data.dial.number} zoom={zoom} primary compact />}</div>} /></>;
  }
  if (state.phase === 'loading' && previewItem) return <div className="cq-detail-scroll"><RowPreview item={previewItem} ownerName={previewOwner} /></div>;
  return <div className="cq-detail-scroll"><DetailMessage state={state} reload={reload} /></div>;
}

/** 案件の概要 (会社・案件・ステージ・架ける番号・通話の様子)。親が架電結果の入力のたびに描き直しても、props が同じなら描き直さない */
export const DealOverview = memo(DealOverviewImpl);
