export interface IndexProgress {
  read_count: number;
  estimated_total: number | null;
  retry_after_seconds: number | null;
}

/** 一覧が初めてそろうまでの進み具合。ホームと求人一覧で同じ数字を表示する。 */
export function ListingPreparationNotice({ progress }: { progress?: IndexProgress | undefined }) {
  const count = (progress?.read_count ?? 0).toLocaleString('ja-JP');
  return <div className="jc-index-preparing" role="status">
    <strong>求人の一覧を準備しています</strong>
    {progress && <p>{progress.estimated_total !== null ? `約 ${progress.estimated_total.toLocaleString('ja-JP')} 件中 ${count} 件まで読みました` : `${count} 件まで読みました`}</p>}
    {progress?.retry_after_seconds != null ? <p>読み込みに失敗したため、{Math.max(1, Math.ceil(progress.retry_after_seconds / 60))} 分後にもう一度読み込みます。</p> : <p>準備が終わると自動で表示します。</p>}
    <progress aria-label="求人の一覧を準備中" />
  </div>;
}
