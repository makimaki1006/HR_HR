import { formatDateTimeJst } from './format';

/** 集計の対象期間を取り違えないよう、古い結果には日時を添える。 */
export function ReportFreshness({ builtAt, stale, refreshing }: { builtAt?: string | null | undefined; stale?: boolean | undefined; refreshing?: boolean | undefined }) {
  if (!builtAt || !stale) return null;
  return <p role="status">{formatDateTimeJst(builtAt, '日時不明')} 時点の集計です。{refreshing ? '新しい集計を作っています。' : '新しい集計を作れなかったため、前の結果を表示しています。'}</p>;
}
