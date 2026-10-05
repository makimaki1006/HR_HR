import type { JobCopyRecord } from './data';
import './job-analysis.css';

export function MarketFactors({ job }: { job: JobCopyRecord }) {
  return <section className="jc-analysis jc-internal-market" aria-label="応募獲得の要因・仮説"><h2>応募獲得の要因・仮説</h2>
    <p>応募推移・市場・比較結果を踏まえて、次に確認する点を整理します。複数の変数が同時に変わるため、応募数の変化だけで原因を確定しません。</p>
    <h3>本文・画像の観測履歴</h3><p>観測日は確認できた日です。実際の変更日とは限りません。応募月と見比べても、この時点で変更効果は確定できません。</p>
    {job.versions.some(version => version.kind === 'published') ? <ul>{job.versions.filter(version => version.kind === 'published').map(version => <li key={version.id}>{version.observedAt.slice(0, 10)} · {version.label} · 画像参照 {version.images?.length ?? version.imageReferences?.length ?? 0}件</li>)}</ul> : <p>掲載観測版は未取得です。</p>}
    <h3>同時に確認する変数</h3><p>文面・画像、給与や勤務条件、課金額・表示量、職種と地域の市場環境、掲載期間、選考の連絡速度、会社の認知・評判を並べて確認します。</p>
    <label className="jc-factor-note">会社のブランド力の根拠<textarea rows={4} placeholder="企業名検索、採用ページの閲覧、応募理由の言及、認知調査など。取得日と出典も記録" /></label>
    <p>ブランド力の実測値は未接続です。このメモは画面内だけに保持され、求人変更・再読み込みで消えます。</p>
  </section>;
}
