import { formatDateTimeJst } from './format';
import type { SavedNote } from './savedNotes';
import './saved-note.css';

interface Props { note: SavedNote; dirty: boolean; onSave: () => void }

/** 記録欄の下に置く保存の行。最後に保存した人と時刻、保存できないときの 1 行を出す。 */
export function SavedNoteBar({ note, dirty, onSave }: Props) {
  if (note.status === 'off') return <p className="jc-note-bar jc-no-print" role="note">操作デモ・媒体CSVの求人では保存されません。入力はこの画面の中だけに残ります。</p>;
  const problem = note.saveError || (note.status === 'unavailable' ? 'いまは保存できません。入力はこの画面に残っています。' : '');
  return <div className="jc-note-bar jc-no-print" role="group" aria-label="記録の保存">
    <button type="button" className="jc-button jc-primary" disabled={!dirty || note.saving || note.status === 'loading'} onClick={onSave}>{note.saving ? '保存中…' : '保存'}</button>
    <span role="status">{note.status === 'loading' ? '保存済みの記録を確認しています。' : note.meta ? `最後に保存：${note.meta.updatedBy} ${formatDateTimeJst(note.meta.updatedAt, '時刻不明')}` : 'まだ保存されていません。'}{dirty && note.status !== 'loading' ? ' 保存していない変更があります。' : ''}</span>
    {problem && <span className="jc-error" role="alert">{problem}</span>}
  </div>;
}
