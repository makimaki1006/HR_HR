import { useCallback, useEffect, useRef, useState } from 'react';
import { apiGet, apiPut, AuthRequiredError, ApiHttpError } from '../../api/client';
import type { JobCopyRecord } from './data';

/** 保存する欄の種類。サーバー (saved_notes.rs) の欄名と同じ。 */
export type NoteField = 'review' | 'factor' | 'incoming';

/** 値の上限 (文字数)。サーバーの上限と同じ。 */
export const NOTE_LIMITS: Record<NoteField, number> = { review: 10_000, factor: 20_000, incoming: 101_000 };

interface WireNote { value: string; updated_by: string; updated_at: string }
export interface NoteMeta { updatedBy: string; updatedAt: string }

/** 求人の記録を保存できるのは、HubSpot の求人 (数字の ID) を開いているときだけ。デモ・媒体CSVだけの求人は保存しない。 */
export function persistableListingId(job: Pick<JobCopyRecord, 'hubspotId' | 'dataSource'>): string | null {
  return job.dataSource === 'hubspot' && job.hubspotId && /^\d{1,30}$/u.test(job.hubspotId) ? job.hubspotId : null;
}

export type NoteStatus = 'off' | 'loading' | 'ready' | 'unavailable';
export interface SavedNote {
  status: NoteStatus;
  /** 最後に保存された内容。保存が無い・読めていないときは null。 */
  value: string | null;
  meta: NoteMeta | null;
  saving: boolean;
  /** 直近の保存の失敗 (空なら失敗なし)。 */
  saveError: string;
  save: (value: string) => Promise<boolean>;
}

function failureText(error: unknown): string {
  if (error instanceof AuthRequiredError) return 'Googleでログインし直してから保存してください。入力はこの画面に残っています。';
  if (error instanceof ApiHttpError && error.status === 413) return '長すぎるため保存できません。短くしてください。入力はこの画面に残っています。';
  return 'いまは保存できません。入力はこの画面に残っています。';
}

/** 求人 1 件の記録欄 1 つの読み書き。listingId が null (デモなど) のときは何も読まず、保存もしない。 */
export function useSavedNote(listingId: string | null, field: NoteField): SavedNote {
  const [status, setStatus] = useState<NoteStatus>(listingId === null ? 'off' : 'loading');
  const [value, setValue] = useState<string | null>(null);
  const [meta, setMeta] = useState<NoteMeta | null>(null);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState('');
  const alive = useRef(true);
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  useEffect(() => {
    if (listingId === null) return;
    const run = { cancelled: false };
    void (async () => {
      const result = await apiGet<{ notes?: Partial<Record<NoteField, WireNote>> }>(`/api/job-copy/listings/${encodeURIComponent(listingId)}/notes`);
      if (run.cancelled) return;
      if (!result.ok) { setStatus('unavailable'); return; }
      const note = result.data.notes?.[field];
      setValue(note ? note.value : null);
      setMeta(note ? { updatedBy: note.updated_by, updatedAt: note.updated_at } : null);
      setStatus('ready');
    })().catch(() => { if (!run.cancelled) setStatus('unavailable'); });
    return () => { run.cancelled = true; };
  }, [listingId, field]);
  const save = useCallback(async (next: string): Promise<boolean> => {
    if (listingId === null) return false;
    setSaving(true); setSaveError('');
    const result = await apiPut<WireNote & { field: string }>(`/api/job-copy/listings/${encodeURIComponent(listingId)}/notes/${field}`, { value: next });
    if (!alive.current) return result.ok;
    setSaving(false);
    if (!result.ok) { setSaveError(failureText(result.error)); return false; }
    setValue(result.data.value); setMeta({ updatedBy: result.data.updated_by, updatedAt: result.data.updated_at }); setStatus('ready');
    return true;
  }, [listingId, field]);
  return { status, value, meta, saving, saveError, save };
}
