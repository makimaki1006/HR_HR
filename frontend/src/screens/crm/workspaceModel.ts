import type { WorkspaceActivity } from '../../generated/WorkspaceActivity';
import type { WorkspacePartial } from '../../generated/WorkspacePartial';
import { errorMessage } from './queueModel';

export const ACTIVITY_KIND_LABELS: Record<string, string> = { call: '通話', note: 'メモ', email: 'メール', meeting: 'ミーティング' };
export type ActivityKindFilter = 'all' | 'call' | 'note' | 'email' | 'meeting';
export const ACTIVITY_FILTERS: readonly { value: ActivityKindFilter; label: string }[] = [
  { value: 'all', label: 'すべて' }, { value: 'call', label: '通話' }, { value: 'note', label: 'メモ' },
  { value: 'email', label: 'メール' }, { value: 'meeting', label: 'ミーティング' },
];

export function filterActivities(items: readonly WorkspaceActivity[], kind: ActivityKindFilter): WorkspaceActivity[] {
  return kind === 'all' ? [...items] : items.filter(a => a.kind === kind);
}

const JST = new Intl.DateTimeFormat('ja-JP', {
  timeZone: 'Asia/Tokyo', year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit', hour12: false,
});

/** HubSpot の時刻 (ISO 8601 または epoch ms) を日本時間 `YYYY/MM/DD HH:mm` にする。読めなければ null */
export function formatTimestamp(raw: string | null): string | null {
  if (!raw) return null;
  const t = raw.trim();
  const ms = /^\d{12,13}$/.test(t) ? Number(t) : Date.parse(t);
  if (!Number.isFinite(ms)) return null;
  const parts = Object.fromEntries(JST.formatToParts(new Date(ms)).map(p => [p.type, p.value]));
  const hour = parts.hour === '24' ? '00' : parts.hour;
  return `${String(parts.year)}/${String(parts.month)}/${String(parts.day)} ${String(hour)}:${String(parts.minute)}`;
}

/** 通話時間 (ミリ秒) → `m分ss秒`。HubSpot の `hs_call_duration` はミリ秒 */
export function formatDurationMs(ms: number | null): string | null {
  if (ms === null) return null;
  const s = Math.round(ms / 1000);
  return s >= 60 ? `${String(Math.floor(s / 60))}分${String(s % 60).padStart(2, '0')}秒` : `${String(s)}秒`;
}

/** 秒 → `mm:ss` */
export function clock(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  return `${String(Math.floor(s / 60)).padStart(2, '0')}:${String(s % 60).padStart(2, '0')}`;
}

const DIRECTION_LABELS: Record<string, string> = { INBOUND: '着信', OUTBOUND: '発信', INCOMING: '着信', OUTGOING: '発信', EMAIL: '送信', INCOMING_EMAIL: '受信', FORWARDED_EMAIL: '転送' };
export const directionLabel = (d: string | null): string | null => (d === null ? null : (DIRECTION_LABELS[d.toUpperCase()] ?? d));

/**
 * HubSpot の活動の状態 (hs_call_status / hs_email_status / hs_meeting_outcome) → 日本語。
 * 知らない英字の値は生のまま出さず「状態: その他」にする (HubSpot で独自に作った日本語の値はそのまま)
 */
const ACTIVITY_STATUS_LABELS: Record<string, string> = {
  COMPLETED: '完了', NO_ANSWER: '応答なし', BUSY: '話し中', FAILED: '失敗', CANCELED: 'キャンセル', CANCELLED: 'キャンセル',
  IN_PROGRESS: '通話中', RINGING: '呼び出し中', CONNECTING: '接続中', QUEUED: '待機中', CALLING_CRM_USER: '発信準備中', MISSED: '不在着信',
  SENT: '送信済み', SENDING: '送信中', SCHEDULED: '予定', BOUNCED: '不達', DELIVERED: '配信済み',
  RESCHEDULED: '日程変更', NO_SHOW: '不参加',
};
export function activityStatusLabel(s: string | null): string | null {
  if (s === null || s.trim() === '') return null;
  const key = s.trim().toUpperCase();
  return ACTIVITY_STATUS_LABELS[key] ?? (/^[A-Z0-9_ -]+$/.test(key) ? '状態: その他' : s);
}

/** 取得できなかった部分の名前 (サーバの part) → 日本語。架電キューの partial.failed にも使う */
export const PARTIAL_LABELS: Record<string, string> = {
  contacts: '担当者', companies: '会社', associations: '担当者・会社との関連', stage_labels: 'ステージ名',
  calls_via_contacts: '担当者経由の通話', calls: '通話', notes: 'メモ', emails: 'メール', meetings: 'ミーティング',
};

/** 取得できなかった理由 (error_kind) → 日本語。コードは画面に出さない */
const PARTIAL_REASONS: Record<string, string> = {
  hubspot_auth: 'HubSpot の読み取り権限が不足しています',
  hubspot_rate_limited: 'HubSpot が混み合っています。少し待ってから開き直してください',
  hubspot_timeout: 'HubSpot の応答が時間内に返りませんでした',
  crm_timeout: '取得に時間がかかりすぎたため途中で止めました',
  hubspot_upstream: 'HubSpot との通信に失敗しました',
  hubspot_transport: 'HubSpot との通信に失敗しました',
  hubspot_decode: 'HubSpot の応答を読み取れませんでした',
  not_found: 'HubSpot に見つかりませんでした',
  not_configured: 'HubSpot への接続が設定されていません',
};

export function partialNotes(partial: readonly WorkspacePartial[]): string[] {
  return partial.map(p => `${PARTIAL_LABELS[p.part] ?? 'その他の情報'}を取得できませんでした(${PARTIAL_REASONS[p.error_kind] ?? '理由は不明です'})`);
}

/** 詳細取得の失敗文言 */
export function detailErrorMessage(kind: string | null, status: number | null): string {
  if (kind === 'forbidden_record') return 'この案件を表示する権限がありません。';
  if (kind === 'not_found') return 'HubSpot にこの案件がありません(削除された可能性があります)。';
  if (kind === 'invalid_id') return '案件の指定が正しくありません。';
  return errorMessage(kind, status);
}
