import type { JobCopyRecord } from './data';

/**
 * 「市場（Indeed）と比べる範囲」の職種を決める場所。
 * いまは自動で選んだ市場職種をそのまま表示する。選び方の画面は、このコンポーネントの中身を差し替えて作る。
 */
export function MarketOccupationSlot({ listing, marketTitle, prefecture }: { listing: JobCopyRecord; marketTitle: string | null; prefecture: string | null }) {
  return <div className="jc-market-occupation" data-listing={listing.id}>
    <dl>
      <dt>地域</dt><dd>{prefecture ?? '未選択'}</dd>
      <dt>職種</dt><dd>{marketTitle ?? '自動では決まりませんでした'}</dd>
    </dl>
  </div>;
}
