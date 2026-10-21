import { useState } from 'react';
import type { JobCopyRecord } from './data';
import { SavedNoteBar } from './SavedNoteBar';
import { NOTE_LIMITS, persistableListingId, useSavedNote } from './savedNotes';
import { formatDateJst } from './format';
import './job-analysis.css';

export function MarketFactors({ job }: { job: JobCopyRecord }) {
  const note = useSavedNote(persistableListingId(job), 'factor');
  // 入力していない間は保存済みのメモを出す。入力を始めたらその内容を保つ (保存済みの内容で上書きしない)。
  const [typed, setTyped] = useState<string | null>(null);
  const memo = typed ?? note.value ?? '';
  const dirty = typed !== null && typed !== (note.value ?? '');
  return <section className="jc-analysis jc-internal-market" aria-label="応募獲得の要因・仮説"><h2>応募獲得の要因・仮説</h2>
    <p>応募推移・市場・比較結果を踏まえて、次に確認する点を整理します。複数の変数が同時に変わるため、応募数の変化だけで原因を確定しません。</p>
    <h3>本文・画像を確認した履歴</h3><p>本文・画像を確認した日で、実際に変更した日とは限りません。応募月と見比べても、変更が応募の増減の原因かどうかは分かりません。</p>
    {job.versions.some(version => version.kind === 'published') ? <ul>{job.versions.filter(version => version.kind === 'published').map(version => <li key={version.id}>{formatDateJst(version.observedAt, '日付不明')} · {version.label} · 画像 {version.images === undefined && version.imageReferences === undefined ? '未取得' : `${String(version.images?.length ?? version.imageReferences?.length ?? 0)}点`}</li>)}</ul> : <p>掲載を確認できた本文は未取得です。</p>}
    <h3>同時に確認する変数</h3><p>文面・画像、給与や勤務条件、課金額・表示量、職種と地域の市場環境、掲載期間、選考の連絡速度、会社の認知・評判を並べて確認します。</p>
    <label className="jc-factor-note">会社のブランド力の根拠<textarea rows={4} maxLength={NOTE_LIMITS.factor} value={memo} onChange={event => { setTyped(event.target.value); }} placeholder="企業名検索、採用ページの閲覧、応募理由の言及、認知調査など。確認した日と出典も記録" /></label>
    <SavedNoteBar note={note} dirty={dirty} onSave={() => { void note.save(memo); }} />
    <p>ブランド力の実測値は未接続です。{note.status === 'off' ? 'このメモは画面内だけに保持され、求人の切り替え・再読み込みで消えます。' : '「保存」を押すと、この求人のメモとして残り、案件管理を開ける人に表示されます。HubSpotには保存しません。'}</p>
  </section>;
}
