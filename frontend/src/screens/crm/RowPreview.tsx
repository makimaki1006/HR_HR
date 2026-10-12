import type { CallQueueItem } from '../../generated/CallQueueItem';
import { formatPhoneForDisplay } from './phone';
import { dateValue } from './queueModel';

const ymd = (raw: string | null) => dateValue(raw)?.replaceAll('-', '/') ?? null;

/**
 * 詳細を読み込む間、一覧の行がすでに持っている値だけで出す見出し (会社・担当者・番号・ステージ・次回/最終架電・担当)。
 * 詳細が届くまでの仮の表示なので、一覧の行にある値だけを使い、発信や編集のボタンは出さない。
 */
export function RowPreview({ item, ownerName }: { item: CallQueueItem; ownerName?: string | undefined }) {
  const phone = formatPhoneForDisplay(item.phone);
  const next = ymd(item.next_call_date);
  const last = ymd(item.last_call_date);
  return <div className="cq-preview" data-testid="row-preview" data-deal-id={item.deal_id} aria-label="一覧の情報(詳細を読み込み中)">
    <h2 className="cq-preview-company">{item.company?.name ?? '(会社名なし)'}</h2>
    <p className="cq-preview-line">
      <span>{item.contact?.name ?? '(担当者なし)'}</span>
      {phone && <><span className="cq-sep" aria-hidden="true">·</span><span className="cq-phone" title={item.phone ?? undefined}>{phone}</span></>}
      <span className="cq-stage">{item.stage_label ?? '(ステージ不明)'}</span>
    </p>
    <p className="cq-preview-line">
      <span>次回 {next ?? 'なし'}{next && item.next_call_time ? ` ${item.next_call_time}` : ''}</span>
      <span>最終 {last ?? '未架電'}</span>
      <span>担当 {item.owner_id ? (ownerName ?? '担当あり') : 'なし'}</span>
    </p>
    <p role="status" className="cq-loading cq-preview-loading">詳細を読み込み中…</p>
  </div>;
}
