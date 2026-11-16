import { useState } from 'react';
import type { JobCopyRecord } from './data';
import { SavedNoteBar } from './SavedNoteBar';
import { NOTE_LIMITS, persistableListingId, useSavedNote } from './savedNotes';
import './job-analysis.css';

export function MarketFactors({ job }: { job: JobCopyRecord }) {
  const note = useSavedNote(persistableListingId(job), 'factor');
  // 入力していない間は保存済みのメモを出す。入力を始めたらその内容を保つ (保存済みの内容で上書きしない)。
  const [typed, setTyped] = useState<string | null>(null);
  const memo = typed ?? note.value ?? '';
  const dirty = typed !== null && typed !== (note.value ?? '');
  return <section className="jc-analysis jc-internal-market" aria-label="この求人のメモ"><h2>メモ</h2>
    <label className="jc-factor-note"><span className="jc-visually-hidden">メモ</span><textarea rows={6} maxLength={NOTE_LIMITS.factor} value={memo} onChange={event => { setTyped(event.target.value); }} placeholder="例：社名で検索される回数、採用ページの閲覧、応募理由で会社の名前が出たか。確かめた日と出どころも書く" /></label>
    <SavedNoteBar note={note} dirty={dirty} onSave={() => { void note.save(memo); }} />
  </section>;
}
