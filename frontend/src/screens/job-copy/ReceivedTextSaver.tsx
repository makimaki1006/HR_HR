import { useEffect, useRef } from 'react';
import type { JobCopyRecord } from './data';
import { SavedNoteBar } from './SavedNoteBar';
import { persistableListingId, useSavedNote } from './savedNotes';

interface Props { job: JobCopyRecord; source: string; incoming: string; onLoad: (source: string, text: string) => void }

/** 保存する形。資料名と受け取った文面。 */
const serialize = (source: string, text: string) => JSON.stringify({ source, text });
function parse(value: string | null): { source: string; text: string } | null {
  if (value === null) return null;
  try {
    const raw = JSON.parse(value) as { source?: unknown; text?: unknown };
    return typeof raw.source === 'string' && typeof raw.text === 'string' ? { source: raw.source, text: raw.text } : null;
  } catch { return null; }
}

/** 「受け取った文面」の下の保存の行。保存済みの文面は、まだ貼り付けていないときだけ入力欄に読み込む。 */
export function ReceivedTextSaver({ job, source, incoming, onLoad }: Props) {
  const note = useSavedNote(persistableListingId(job), 'incoming');
  const applied = useRef(false);
  const saved = parse(note.value);
  useEffect(() => {
    if (applied.current || note.status !== 'ready') return;
    applied.current = true;
    if (saved && incoming === '') onLoad(saved.source, saved.text);
  }, [note.status, saved, incoming, onLoad]);
  const dirty = saved === null ? incoming !== '' : serialize(source, incoming) !== serialize(saved.source, saved.text);
  return <SavedNoteBar note={note} dirty={dirty} onSave={() => { void note.save(serialize(source, incoming)); }} />;
}
