import { memo, useState } from 'react';
import type { WorkspaceActivity } from '../../generated/WorkspaceActivity';
import type { WorkspaceResponse } from '../../generated/WorkspaceResponse';
import { formatPhoneForDisplay, toDomesticPhone } from './phone';
import { toE164Jp } from './smartEmbed';
import type { DialResult, ZoomPhone } from './useZoomPhone';
import type { DetailState } from './useDealDetail';
import {
  ACTIVITY_FILTERS, ACTIVITY_KIND_LABELS, activityStatusLabel, directionLabel, filterActivities, formatDurationMs, formatTimestamp, partialNotes,
} from './workspaceModel';
import type { ActivityKindFilter } from './workspaceModel';
import { FRESHNESS_TICK_MS, clock, freshnessLabel } from './workspaceModel';
import { RESULT_LABELS, useTicking } from './ZoomPhonePanel';
import { dealJobSearchUrl, extractUrls, isEmptyGoogleSearch, safeHttpUrl } from './centerLinks';
import { PropLink } from './CenterTabs';
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
};

/**
 * 電話番号 1 つ分: 表示(ハイフン区切り) / 発信(Smart Embed、元の値) / コピー(国内形式の数字) / tel:
 * `primary` は「架ける番号」の大きい表示
 */
export function PhoneRow({ label, raw, zoom, primary = false, compact = false }: {
  label: string; raw: string; zoom: ZoomPhone; primary?: boolean;
  /** 1 行の表示 (番号と発信だけ。コピーと端末の電話は「詳しく表示」で出す) */
  compact?: boolean;
}) {
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
  if (compact) {
    return <span className="wd-phone wd-phone-primary wd-phone-compact">
      <span className="cq-phone" title={`${label}: ${raw}`}>{shown}</span>
      <button type="button" className="wd-dial" onClick={dial} disabled={e164 === null || zoom.embed === 'disabled'}
        aria-label={`${label} ${shown} に発信`}>発信</button>
      {e164 === null && <small className="crm-muted">ダイヤルできる形式ではありません</small>}
      {note && <small role="status">{note}</small>}
    </span>;
  }
  return <div className={`wd-phone${primary ? ' wd-phone-primary' : ''}`}>
    <span className="wd-phone-label">{label}</span>
    <span className="cq-phone" title={raw}>{shown}</span>
    <span className="wd-phone-actions">
      <button type="button" className="wd-dial" onClick={dial} disabled={e164 === null || zoom.embed === 'disabled'}
        aria-label={`${label} ${shown} に発信`}>発信</button>
      <button type="button" className="wd-copy" onClick={copy} aria-label={`${label} ${shown} の番号をコピー`}>番号をコピー</button>
      {e164 !== null && <a href={`tel:${e164}`} aria-label={`${label} ${shown} へ端末の電話で発信`}
        title="このパソコン・スマートフォンの電話アプリで発信します(tel: リンク)">端末の電話で発信</a>}
    </span>
    {e164 === null && <small className="crm-muted">ダイヤルできる形式ではありません</small>}
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

function ActivityItem({ a, ownerNames }: { a: WorkspaceActivity; ownerNames: ReadonlyMap<string, string> }) {
  const when = formatTimestamp(a.timestamp);
  const who = a.owner_id ? (ownerNames.get(a.owner_id) ?? null) : null;
  const dir = directionLabel(a.direction);
  const status = activityStatusLabel(a.status);
  const dur = formatDurationMs(a.duration_ms);
  return <li className={`wd-act wd-act-${a.kind}`}>
    <div className="wd-act-head">
      <span className="crm-status">{ACTIVITY_KIND_LABELS[a.kind] ?? a.kind}</span>
      {a.title && <strong>{a.title}</strong>}
      <small>{when ?? '日時不明'}</small>
      {who && <small className="wd-act-who">{who}</small>}
    </div>
    {(dir !== null || status !== null || dur !== null || a.via === 'contact') && <p className="wd-act-meta">
      {[dir, status, dur && `通話時間 ${dur}`, a.via === 'contact' && '担当者の通話(別の案件のものを含む場合があります)'].filter(Boolean).join(' · ')}
    </p>}
    {a.body && <p className="wd-act-body">{a.body}</p>}
  </li>;
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

function Overview({ data, zoom, stopLabel, callBar, onOpenZoom, refreshing, refreshError, onRefresh, density, stageMove }: {
  data: WorkspaceResponse; zoom: ZoomPhone; stopLabel: StopLabel;
  callBar?: CallBarInfo | null | undefined; onOpenZoom?: (() => void) | undefined;
  refreshing: boolean; refreshError: string | null; onRefresh?: (() => void) | undefined;
  density?: OverviewDensity | undefined;
  stageMove?: StageMoveCtx | null | undefined;
}) {
  const d = data.deal;
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
        <div className="wd-line">
          <h2 className="wd-line-company" title={d.name ?? undefined}>{company?.name ?? d.name ?? '(名称なし)'}</h2>
          <span className="wd-line-sep" aria-hidden="true">·</span>
          <span className="wd-line-contact">{contact?.name ?? <span className="crm-muted">担当者なし</span>}</span>
          <span className="wd-line-sep" aria-hidden="true">·</span>
          {data.dial ? <PhoneRow label={SOURCE_LABELS[data.dial.source] ?? '電話'} raw={data.dial.number} zoom={zoom} primary compact />
            : <span className="crm-muted">番号を確認できません</span>}
          {status !== null && callBar?.kind !== 'failed' && <span className={`wd-line-status wd-callbar-${callBar?.kind ?? ''}`} role="status" data-testid="call-bar-status">
            <strong>{status.label}</strong>{status.extra && <span> {status.extra}</span>}</span>}
          {status !== null && callBar?.kind !== 'failed' && onOpenZoom && <button type="button" className="wd-callbar-open" onClick={onOpenZoom}
            title="消音・保留・通話を切る・数字の入力は Zoom の枠で行います">Zoomを開く</button>}
          {stopReasons.length > 0 && <span className="cq-flag wd-line-flag" title={stopReasons.join(' / ')}>架電の注意 {String(stopReasons.length)} 件</span>}
          {notes.length > 0 && <span className="cq-flag wd-line-flag" title={notes.join(' / ')}>一部の情報が欠けています</span>}
          <span className="wd-line-end">
            <a href={d.deep_link} target="_blank" rel="noreferrer">HubSpotで開く</a>
            <DensityToggle density={density} />
          </span>
        </div>
        {/* 発信できなかったときは、何をすればよいかを 1 行の下に出す */}
        {callBar?.kind === 'failed' && <CallBar info={callBar} now={zoom.now} onOpenZoom={onOpenZoom} />}
      </div>
    </article>;
  }
  return <article className="wd" aria-label="架電先の詳細">
    {/* 会社・案件・ステージ・HubSpot と「架ける番号」・通話の様子 (置いた列の上端に固定) */}
    <div className="wd-top">
      <header className="wd-head">
        <div className="wd-head-main">
          <h2>{company?.name ?? d.name ?? '(名称なし)'}</h2>
          <p>{d.name ?? '(案件名なし)'}</p></div>
        <div className="wd-head-side">
          <StageMover ctx={stageMove} fallback={<span className="cq-stage">{d.stage_label ?? '(ステージ名を取得できません)'}</span>} />
          <a href={d.deep_link} target="_blank" rel="noreferrer">HubSpotで開く</a>
          {density && <DensityToggle density={density} />}
        </div>
      </header>
      <Freshness fetchedAt={data.fetched_at} now={zoom.now} refreshing={refreshing} refreshError={refreshError} onRefresh={onRefresh} />

      <section className="wd-dialbox" aria-label="架ける番号">
        <h3 className="wd-dial-title">架ける番号</h3>
        {data.dial ? <PhoneRow label={SOURCE_LABELS[data.dial.source] ?? '電話'} raw={data.dial.number} zoom={zoom} primary />
          : <p className="crm-muted">番号を確認できません。担当者・会社の情報を HubSpot で確認してください。</p>}
        {otherPhones.length > 0 && <details className="wd-other"><summary>ほかの番号({otherPhones.length})</summary>
          <div className="wd-other-list">{otherPhones.map(p => <PhoneRow key={p.key} label={p.label} raw={p.raw} zoom={zoom} />)}</div></details>}
      </section>
      {callBar && <CallBar info={callBar} now={zoom.now} onOpenZoom={onOpenZoom} />}
      {stopReasons.length > 0 && <ul className="wd-stop" aria-label="架電の注意">{stopReasons.map(s => <li key={s} className="cq-flag">{s}</li>)}</ul>}
      {notes.length > 0 && <div className="cq-notice cq-warn wd-notes" role="status"><strong>一部の情報が欠けています</strong>
        <ul>{notes.map(n => <li key={n}>{n}</li>)}</ul></div>}
    </div>
  </article>;
}

/** 活動ログ (いつ・誰が・どうだったか)。種類で絞り込める */
function ActivityLogImpl({ data, placeholder, ownerNames }: {
  data: WorkspaceResponse | null; placeholder: string; ownerNames: ReadonlyMap<string, string>;
}) {
  const [kind, setKind] = useState<ActivityKindFilter>('all');
  if (data === null) return <div className="dock-scroll"><p className="dock-placeholder">{placeholder}</p></div>;
  const acts = filterActivities(data.activities, kind);
  return <section className="wd-log dock-scroll" aria-label="活動ログ" data-testid="activity-log">
    <p className="wd-log-head"><strong>{data.activities.length} 件</strong>{data.activities_truncated && <small>(これより古い履歴は表示していません)</small>}</p>
    <p className="wd-scope">{data.activity_scope}</p>
    <div className="wd-filters" role="group" aria-label="活動の種類">
      {ACTIVITY_FILTERS.map(f => <button key={f.value} type="button" aria-pressed={kind === f.value} onClick={() => { setKind(f.value); }}>{f.label}</button>)}
    </div>
    {acts.length === 0 && <p className="crm-muted">{data.activities.length === 0 ? '表示できる活動履歴はありません。' : 'この種類の活動はありません。'}</p>}
    <ul className="wd-acts">{acts.map(a => <ActivityItem key={`${a.kind}-${a.id}`} a={a} ownerNames={ownerNames} />)}</ul>
  </section>;
}

/** 親 (架電画面) が描き直しても、props が同じなら描き直さない */
export const ActivityLog = memo(ActivityLogImpl);

/**
 * 案件・会社のリンク (求人検索・ホームページ・求人票・求人媒体)。クリックで「求人検索・リンク先」パネルのタブに開く。
 * 求人検索は「URL_求人検索」、無ければ架ける番号で検索する URL
 */
function DealLinksImpl({ data }: { data: WorkspaceResponse }) {
  const d = data.deal;
  const company = data.companies.find(c => c.is_primary) ?? null;
  const search = dealJobSearchUrl(data);
  const homepage = safeHttpUrl(d.homepage_url)?.toString() ?? null;
  const site = safeHttpUrl(company?.website)?.toString() ?? null;
  const posting = d.job_posting_url !== null && !isEmptyGoogleSearch(d.job_posting_url) ? safeHttpUrl(d.job_posting_url)?.toString() ?? null : null;
  const media = extractUrls(d.media_job_urls);
  const rows: { key: string; dt: string; url: string; label: string; note?: string }[] = [];
  if (search !== null) rows.push({ key: 'search', dt: '求人検索', url: search, label: '求人検索',
    ...(safeHttpUrl(d.job_search_url) === null ? { note: '(架ける番号で検索)' } : {}) });
  if (homepage !== null) rows.push({ key: 'home', dt: 'ホームページ', url: homepage, label: 'ホームページ' });
  if (site !== null && site !== homepage) rows.push({ key: 'site', dt: '会社のサイト', url: site, label: '会社のサイト' });
  if (posting !== null) rows.push({ key: 'posting', dt: '求人票', url: posting, label: '求人票' });
  media.forEach((u, i) => { rows.push({ key: `media-${String(i)}`, dt: media.length > 1 ? `求人媒体 ${String(i + 1)}` : '求人媒体', url: u, label: media.length > 1 ? `求人媒体 ${String(i + 1)}` : '求人媒体' }); });
  return <section className="wd-card" aria-label="リンク">
    <h3>リンク<small>クリックすると、このパネルの中に開きます</small></h3>
    {rows.length === 0 ? <p className="crm-muted">登録されたリンクはありません。</p>
      : <dl className="wd-links">{rows.map(r => <div key={r.key}><dt>{r.dt}</dt>
        <dd><PropLink url={r.url} label={r.label}>{r.key === 'search' ? <>求人を検索する{r.note && <small> {r.note}</small>}</> : r.url}</PropLink></dd></div>)}</dl>}
  </section>;
}

/** 親が描き直しても、案件が同じなら描き直さない */
export const DealLinks = memo(DealLinksImpl);

function DetailMessage({ state, reload }: { state: DetailState; reload: () => void }) {
  if (state.phase === 'idle') return <div className="cq-notice cq-empty wd-empty"><strong>架電一覧から架電先を選んでください</strong>
    <p>案件・担当者・会社・活動履歴を HubSpot から読み込みます。</p></div>;
  if (state.phase === 'loading') return <p role="status" className="cq-loading">詳細を読み込み中…</p>;
  if (state.phase === 'forbidden') return <div className="cq-notice cq-error" role="alert"><strong>表示できません</strong><p>{state.message}</p></div>;
  return <div className="cq-notice cq-error" role="alert"><strong>詳細を取得できませんでした</strong>
    <p>{state.message || '取得に失敗しました。'}</p><button type="button" onClick={reload}>再試行</button></div>;
}

function DealOverviewImpl({ state, reload, refresh, zoom, stopLabel = rawStopLabel, callBar, onOpenZoom, density, stageMove }: {
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
    return <Overview data={state.data} zoom={zoom} stopLabel={stopLabel} callBar={callBar} onOpenZoom={onOpenZoom}
      refreshing={state.refreshing} refreshError={state.refreshError} onRefresh={refresh} density={density} stageMove={stageMove} />;
  }
  return <div className="cq-detail-scroll"><DetailMessage state={state} reload={reload} /></div>;
}

/** 案件の概要 (会社・案件・ステージ・架ける番号・通話の様子)。親が架電結果の入力のたびに描き直しても、props が同じなら描き直さない */
export const DealOverview = memo(DealOverviewImpl);
