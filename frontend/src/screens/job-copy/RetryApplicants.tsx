import { requestApplicantsRetry } from './listingApplicationDates';

/** 応募を読み込めなかったときの再試行ボタン。開いている求人の応募だけを読み直す。 */
export function RetryApplicants({ id }: { id: string | undefined }) {
  if (!id) return null;
  return <button type="button" className="jc-button" onClick={() => { requestApplicantsRetry(id); }}>もう一度読み込む</button>;
}
