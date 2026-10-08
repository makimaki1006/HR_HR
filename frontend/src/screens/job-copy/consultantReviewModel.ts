import type { JobCopyRecord } from './data';
import { billingConflict, billingEntries, billingOverlaps, formatDay, formatYen, realBilling } from './timelineModel';
import type { BillingEntry } from './timelineModel';
function validDate(value: string): boolean {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value)) return false;
  const parsed = new Date(`${value}T00:00:00Z`);
  return Number.isFinite(parsed.getTime()) && parsed.toISOString().slice(0, 10) === value;
}

export function observationWindowError(start: string, end: string): string | null {
  if (!start && !end) return null;
  if (!start || !end) return '確認開始日と終了日を両方入力してください。';
  if (!validDate(start) || !validDate(end)) return '確認期間に有効な日付を入力してください。';
  return start > end ? '確認終了日は開始日以降にしてください。' : null;
}

/**
 * The 費用と応募単価 text from the real billing only: the HRハッカー実績 and the billing CSV rows
 * the user applied (the same rows the timeline shows). The dummy billing is never used here.
 */
export function reportBillingText(job: JobCopyRecord, billing: readonly BillingEntry[] | undefined): { heading: string; detail: string } {
  const real = realBilling(billingEntries(job, billing, { dummy: false }));
  const csv = real.filter(entry => entry.source === 'csv');
  const hrh = real.some(entry => entry.source === 'hrhacker') || Boolean(job.hrhPerformance);
  const noUnitCost = '本文の版との対応は未確認で、HubSpotの応募総数から応募単価を作りません。';
  if (!csv.length) return hrh
    ? { heading: '媒体の期間別実績を取得済み', detail: `課金・クリック画面で確認できます。${noUnitCost}` }
    : { heading: '課金情報は未取得', detail: '後日受領する課金情報を、対象求人・掲載期間と対応させます。' };
  const first = csv.reduce((day, entry) => entry.start < day ? entry.start : day, csv[0]?.start ?? '');
  const last = csv.reduce((day, entry) => entry.end > day ? entry.end : day, csv[0]?.end ?? '');
  const known = csv.filter(entry => entry.amountYen !== null);
  const total = billingOverlaps(csv) ? '期間が重なる行があるため合計していません'
    : known.length ? `金額の分かる行の合計 ${formatYen(known.reduce((sum, entry) => sum + (entry.amountYen ?? 0), 0))}${known.length < csv.length ? '（金額の記載がない行あり）' : ''}` : '金額の記載なし';
  const conflict = billingConflict(real) ? 'HRハッカーの期間別実績と同じ日を含む行があります（どちらが正しいか決められないため、タイムラインでは合計していません）。' : '';
  return {
    heading: hrh ? '媒体の期間別実績と読み込んだ課金CSVあり' : '読み込んだ課金CSVあり',
    detail: `読み込んだ課金CSV：${String(csv.length)}行・${formatDay(first)}〜${formatDay(last)}・${total}。${conflict}課金CSVはこの画面を開いている間だけ使います。${noUnitCost}`,
  };
}
