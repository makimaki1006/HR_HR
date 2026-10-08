import { memo, useState } from 'react';
import type { WorkspaceActivity } from '../../generated/WorkspaceActivity';
import type { WorkspaceResponse } from '../../generated/WorkspaceResponse';
import { dateValue } from './queueModel';
import { formatPhoneForDisplay, toDomesticPhone } from './phone';
import { toE164Jp } from './smartEmbed';
import type { DialResult, ZoomPhone } from './useZoomPhone';
import type { DetailState } from './useDealDetail';
import {
  ACTIVITY_FILTERS, ACTIVITY_KIND_LABELS, activityStatusLabel, directionLabel, filterActivities, formatDurationMs, formatTimestamp, partialNotes,
} from './workspaceModel';
import type { ActivityKindFilter } from './workspaceModel';
import { clock } from './workspaceModel';
import { RESULT_LABELS, useTicking } from './ZoomPhonePanel';
import { dealJobSearchUrl, extractUrls, isEmptyGoogleSearch, safeHttpUrl } from './centerLinks';
import { PropLink } from './CenterTabs';
import type { ZoomEvent } from './smartEmbed';

/** HubSpot の選択肢の値 → 表示ラベル (不通時チェック bpo_10・ブロック理由 bpo_4)。定義がまだ無いときは値のまま */
export type StopLabel = (property: 'bpo_10' | 'bpo_4', value: string) => string;
export const rawStopLabel: StopLabel = (_p, v) => v;

const SOURCE_LABELS: Record<string, string> = { deal: '案件の番号', contact: '担当者の電話', mobile: '担当者の携帯', company: '会社の電話' };
const ymd = (raw: string | null) => dateValue(raw)?.replaceAll('-', '/') ?? null;

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

export function CallBar({ info, now, onOpenZoom }: { info: CallBarInfo; now: () => number; onOpenZoom?: (() => void) | undefined }) {
  const connectedAt = info.kind === 'connected' ? info.connectedAt : null;
  const t = useTicking(connectedAt !== null, now);
  const who = 'number' in info ? formatPhoneForDisplay(info.number) : null;
  let label: string;
  let extra: string | null = null;
  if (info.kind === 'dialing') label = '発信しています…';
  else if (info.kind === 'failed') label = '発信できませんでした';
  else if (info.kind === 'ringing') label = info.inbound ? '着信中' : '呼び出し中';
  else if (info.kind === 'connected') label = '通話中';
  else {
    label = (info.result ? RESULT_LABELS[info.result] : undefined) ?? '通話が終了しました';
    if (info.talkSeconds !== null) extra = `(通話時間 ${clock(info.talkSeconds)})`;
  }
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

function ActivityItem({ a }: { a: WorkspaceActivity }) {
  const when = formatTimestamp(a.timestamp);
  const dir = directionLabel(a.direction);
  const status = activityStatusLabel(a.status);
  const dur = formatDurationMs(a.duration_ms);
  return <li className={`wd-act wd-act-${a.kind}`}>
    <div className="wd-act-head">
      <span className="crm-status">{ACTIVITY_KIND_LABELS[a.kind] ?? a.kind}</span>
      {a.title && <strong>{a.title}</strong>}
      <small>{when ?? '日時不明'}</small>
    </div>
    {(dir !== null || status !== null || dur !== null || a.via === 'contact') && <p className="wd-act-meta">
      {[dir, status, dur && `通話時間 ${dur}`, a.via === 'contact' && '担当者の通話(別の案件のものを含む場合があります)'].filter(Boolean).join(' · ')}
    </p>}
    {a.body && <p className="wd-act-body">{a.body}</p>}
  </li>;
}

function Detail({ data, zoom, ownerName, stopLabel, callBar, onOpenZoom }: {
  data: WorkspaceResponse; zoom: ZoomPhone; ownerName?: string | undefined; stopLabel: StopLabel;
  callBar?: CallBarInfo | null | undefined; onOpenZoom?: (() => void) | undefined;
}) {
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
          <div className="wd-other-list">{otherPhones.map(p => <PhoneRow key={p.key} label={p.label} raw={p.raw} zoom={zoom} />)}</div></details>}
      </section>
      {callBar && <CallBar info={callBar} now={zoom.now} onOpenZoom={onOpenZoom} />}
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

    <DealLinks data={data} company={company} />

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

/**
 * 案件・会社のリンク (求人検索・ホームページ・求人票・求人媒体)。クリックで中央のタブに開く。
 * 求人検索は「URL_求人検索」、無ければ架ける番号で検索する URL
 */
function DealLinks({ data, company }: { data: WorkspaceResponse; company: WorkspaceResponse['companies'][number] | null }) {
  const d = data.deal;
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
    <h3>リンク<small>クリックすると、この画面の中に開きます</small></h3>
    {rows.length === 0 ? <p className="crm-muted">登録されたリンクはありません。</p>
      : <dl className="wd-links">{rows.map(r => <div key={r.key}><dt>{r.dt}</dt>
        <dd><PropLink url={r.url} label={r.label}>{r.key === 'search' ? <>求人を検索する{r.note && <small> {r.note}</small>}</> : r.url}</PropLink></dd></div>)}</dl>}
  </section>;
}

function DetailMessage({ state, reload }: { state: DetailState; reload: () => void }) {
  if (state.phase === 'idle') return <div className="cq-notice cq-empty wd-empty"><strong>左の一覧から架電先を選んでください</strong>
    <p>案件・担当者・会社・活動履歴を HubSpot から読み込みます。</p></div>;
  if (state.phase === 'loading') return <p role="status" className="cq-loading">詳細を読み込み中…</p>;
  if (state.phase === 'forbidden') return <div className="cq-notice cq-error" role="alert"><strong>表示できません</strong><p>{state.message}</p></div>;
  return <div className="cq-notice cq-error" role="alert"><strong>詳細を取得できませんでした</strong>
    <p>{state.message || '取得に失敗しました。'}</p><button type="button" onClick={reload}>再試行</button></div>;
}

function DealDetailImpl({ state, reload, zoom, ownerName, stopLabel = rawStopLabel, callBar, onOpenZoom }: {
  state: DetailState; reload: () => void; zoom: ZoomPhone; ownerName?: string | undefined; stopLabel?: StopLabel;
  /** 「架ける番号」の下に出す通話の様子 (出すものが無ければ null) */
  callBar?: CallBarInfo | null | undefined;
  /** Zoom の枠を開く */
  onOpenZoom?: (() => void) | undefined;
}) {
  if (state.phase === 'ready' && state.data !== null) {
    return <Detail data={state.data} zoom={zoom} ownerName={ownerName} stopLabel={stopLabel} callBar={callBar} onOpenZoom={onOpenZoom} />;
  }
  return <div className="cq-detail-scroll"><DetailMessage state={state} reload={reload} /></div>;
}

/** 親 (架電画面) が架電結果の入力のたびに描き直しても、props が同じなら描き直さない */
export const DealDetail = memo(DealDetailImpl);
