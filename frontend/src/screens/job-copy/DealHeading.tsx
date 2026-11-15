import type { WorkspaceResponse } from '../../generated/WorkspaceResponse';
import type { DealSelection } from './hubspotListings';
import { OutLink } from './Pressable';
import { useOwnerName } from './ownerNames';
import { todayJst } from './timelineModel';

/** 案件のページの見出しに使う値。契約・成果の欄と案件のまとめが読んだものを受け取る（新しく読みには行かない）。 */
export interface DealHead { company: { id: string; name: string } | null; detail: WorkspaceResponse | null }

/** HubSpot の日付の値（YYYY-MM-DD・ISO・ミリ秒）を YYYY-MM-DD にする。読めなければ null。 */
export function dayOf(value: string | null | undefined): string | null {
  if (!value) return null;
  if (/^\d{4}-\d{2}-\d{2}/.test(value)) return value.slice(0, 10);
  const date = new Date(Number(value));
  return /^\d+$/.test(value) && Number.isFinite(date.getTime()) ? date.toISOString().slice(0, 10) : null;
}
/** 満了日までの日数（今日が満了日なら 0、過ぎていればマイナス）。 */
export function daysUntil(day: string, today = todayJst()): number {
  return Math.round((Date.parse(`${day}T00:00:00Z`) - Date.parse(`${today}T00:00:00Z`)) / 86_400_000);
}
/**
 * 満了までの日数の表示。赤（急ぐ）は残り 30 日以内と、過ぎて 30 日以内だけ。
 * 過ぎて 31 日以上たった案件は「満了済み」と灰色にする（終わった案件を急ぐものに見せない）。
 */
export function expiryText(days: number): { text: string; tone: 'normal' | 'urgent' | 'ended' } {
  if (days >= 0) return { text: `満了まで ${String(days)}日`, tone: days <= 30 ? 'urgent' : 'normal' };
  return -days <= 30 ? { text: `満了から ${String(-days)}日`, tone: 'urgent' } : { text: '満了済み', tone: 'ended' };
}

/** 大見出しは案件名。下に顧客名・担当・ステージ・満了までの日数と HubSpot へのリンク。 */
export function DealHeading({ deal, head }: { deal: DealSelection; head: DealHead }) {
  const values = head.detail?.selected.deal;
  const consultant = useOwnerName(values?.consultant ?? null);
  const expiration = dayOf(values?.contract_expiration_date);
  const expiry = expiration ? expiryText(daysUntil(expiration)) : null;
  const name = deal.name.trim() || (head.detail?.deal.name ?? '案件名未取得');
  const stage = head.detail?.deal.stage_label;
  const parts = [head.company?.name, consultant ? `担当：${consultant}` : null, stage ? `ステージ：${stage}` : null].filter((part): part is string => Boolean(part));
  return <header className="jc-deal-head">
    <h1>{name}</h1>
    <p className="jc-deal-meta">{parts.map((part, index) => <span key={part}>{index > 0 && <span aria-hidden="true" className="jc-deal-sep">・</span>}{part}</span>)}
      {expiry && <span>{parts.length > 0 && <span aria-hidden="true" className="jc-deal-sep">・</span>}<strong className={`jc-deal-expiry${expiry.tone === 'urgent' ? ' is-urgent' : expiry.tone === 'ended' ? ' is-ended' : ''}`} data-testid="deal-expiry">{expiry.text}</strong></span>}
      {head.detail?.deal.deep_link && <OutLink href={head.detail.deal.deep_link} name="HubSpotで案件を開く" />}</p>
  </header>;
}
