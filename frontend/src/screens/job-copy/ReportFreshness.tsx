import { useState } from 'react';
import { formatDateTimeJst } from './format';

const LONG_STALE_MS = 6 * 60 * 60 * 1000;

/** 集計の対象期間を取り違えないよう、古い結果には日時を添える。 */
export function ReportFreshness({ builtAt, stale, refreshing }: { builtAt?: string | null | undefined; stale?: boolean | undefined; refreshing?: boolean | undefined }) {
  const [now] = useState(() => Date.now());
  if (!builtAt || !stale) return null;
  const at = formatDateTimeJst(builtAt, '日時不明');
  // 半日近く古いままなら「作っています」では誤解させるため、作れていないことを伝える。
  const age = now - new Date(builtAt).getTime();
  if (refreshing && Number.isFinite(age) && age >= LONG_STALE_MS) {
    return <p role="status">{at} 時点の集計です。新しい集計がまだ終わっていないため、前の結果を表示しています。数字が古いままのときは管理者に知らせてください。</p>;
  }
  return <p role="status">{at} 時点の集計です。{refreshing ? '新しい集計を作っています。' : '新しい集計を作れなかったため、前の結果を表示しています。'}</p>;
}
