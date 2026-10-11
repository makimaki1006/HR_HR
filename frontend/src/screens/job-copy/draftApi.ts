import { useEffect, useRef, useState } from 'react';
import { apiGet, apiPatch, apiPost, ApiHttpError } from '../../api/client';
import type { DraftSnapshot } from '../../generated/DraftSnapshot';
import type { DraftOperationResponse } from '../../generated/DraftOperationResponse';
export type DraftResult = DraftOperationResponse;
export function draftError(error: Error): string {
  if (error instanceof ApiHttpError) {
    if (error.status === 401 || error.status === 403) return '案を保存する権限がありません。ログインと利用設定を確認してください。';
    if (error.status === 409) return '案が更新されたか、保存できない状態です。求人を再取得して内容を確認してください。';
    if (error.status === 400 || error.status === 413) return '案の内容を保存できません。空欄や文章の長さを確認してください。';
  }
  return '保存結果を確認できませんでした。「保存結果を確認」を押してください。';
}
/** 応答が途切れたときも、同じ操作を照会・再送する。入力が変わっても再送内容は変えない。 */
export function useDraftMutation(onSaved: (draft: DraftSnapshot, revision: string) => void) {
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState('');
  const [uncertain, setUncertain] = useState(false);
  const pending = useRef<{ url: string; method: 'post' | 'patch'; body: Record<string, unknown>; id: string } | null>(null);
  const mounted = useRef(true);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const saved = useRef(onSaved);
  useEffect(() => { saved.current = onSaved; }, [onSaved]);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; if (timer.current) clearTimeout(timer.current); }; }, []);
  async function send(url: string, method: 'post' | 'patch', body: Record<string, unknown>) {
    if (busy) return;
    const operation = pending.current ?? { url, method, body: { ...body, operation_id: crypto.randomUUID() }, id: '' };
    operation.id = String(operation.body.operation_id); pending.current = operation;
    setBusy(true); setMessage('案の保存結果を確認しています…'); setUncertain(false);
    let result = await (operation.method === 'post' ? apiPost<DraftResult> : apiPatch<DraftResult>)(operation.url, operation.body, { timeoutMs: 120_000 });
    if (!mounted.current) return;
    const finish = () => {
      if (!mounted.current) return;
      if (!result.ok) { setMessage(draftError(result.error)); setBusy(false); setUncertain(!(result.error instanceof ApiHttpError && result.error.status < 500)); if (result.error instanceof ApiHttpError && result.error.status < 500) pending.current = null; return; }
      if (result.data.status === 'saved' && result.data.draft && result.data.revision) { saved.current(result.data.draft, result.data.revision); pending.current = null; setMessage('案を保存しました。'); setBusy(false); return; }
      if (result.data.status === 'failed') { pending.current = null; setMessage('案を保存できませんでした。求人を再取得してください。'); setBusy(false); return; }
      setMessage('案の保存を受け付けました。時間を置いて自動で再確認します。');
      timer.current = setTimeout(() => { void (async () => { result = await apiGet<DraftResult>(`/api/job-copy/draft-operations/${encodeURIComponent(operation.id)}`, { timeoutMs: 120_000 }); finish(); })(); }, 2_000);
    };
    finish();
  }
  return { busy, message, uncertain, send };
}
