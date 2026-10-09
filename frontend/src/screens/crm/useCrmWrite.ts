import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { WriteApi } from './crmWrite';
import { buildPatchBody, fieldKey, requestSignature, stageKey } from './writeModel';
import type { SaveRequest } from './writeModel';
import type { EditSchema, PropValues, WriteObject } from './writeTypes';

/** 1 回の保存の状態 (headless-crm-design §12: 緑 = 保存済み / 黄 = 一時保存・HubSpot 反映待ち / 赤 = 保存できていません) */
export type FieldStatus =
  | { phase: 'saving' }
  | { phase: 'saved' }
  /** 一時保存した。`slow` は 10 分たってもまだ反映されていない */
  | { phase: 'queued'; slow: boolean }
  /** 保存できていない。`req` があれば同じ操作 (同じ operation_id) で再試行できる */
  | { phase: 'error'; message: string; req: SaveRequest | null }
  /** 入力の誤り (422)。直して保存し直す */
  | { phase: 'invalid'; message: string };

export type SaveOutcome =
  | { kind: 'saved' | 'queued' | 'conflict' }
  | { kind: 'invalid'; errors: Record<string, string>; missing: string[] }
  | { kind: 'error'; message: string };

/** 保存した値の仮の表示 (読み直した詳細に切り替わるまで。`pending` は HubSpot 反映待ち) */
export interface ValueOverlay { value: string | null; pending: boolean; fetchedAt: string | null }
export interface StageOverlay { pipeline_id: string; stage_id: string; label: string; pending: boolean; fetchedAt: string | null }

/** `object` は HubSpot の現在値が違ったオブジェクト (案件 / 担当者 / 会社)。`current` はそのオブジェクトの項目の現在値 */
export interface ConflictState { req: SaveRequest; object: WriteObject; current: PropValues; changed: string[] }

export type SchemaState =
  | { phase: 'idle' | 'loading' }
  | { phase: 'ready'; schema: EditSchema }
  | { phase: 'error' };

export const POLL_INTERVAL_MS = 10_000;
export const POLL_MAX_MS = 10 * 60_000;

export const WRITES_OFF_NOTE = 'この案件はまだ編集できません（試験運用中）';

export interface UseCrmWriteOptions {
  dealId: string | null;
  api: WriteApi;
  /** 保存が済んだ (HubSpot に反映された) とき。詳細を読み直す */
  onSaved: (dealId: string) => void;
  pollIntervalMs?: number | undefined;
  newId?: (() => string) | undefined;
}

export function useCrmWrite({ dealId, api, onSaved, pollIntervalMs = POLL_INTERVAL_MS, newId }: UseCrmWriteOptions) {
  const [schemaState, setSchemaState] = useState<{ dealId: string | null; s: SchemaState }>({ dealId: null, s: { phase: 'idle' } });
  const [statuses, setStatuses] = useState<Readonly<Record<string, FieldStatus>>>({});
  const [overlay, setOverlay] = useState<Readonly<Record<string, ValueOverlay>>>({});
  const [stageOverlay, setStageOverlay] = useState<Readonly<Record<string, StageOverlay>>>({});
  const [disabled, setDisabled] = useState<ReadonlySet<string>>(() => new Set());
  const [conflict, setConflict] = useState<ConflictState | null>(null);
  const opIds = useRef(new Map<string, string>());
  const timers = useRef(new Set<ReturnType<typeof setTimeout>>());
  const alive = useRef(true);
  const isAlive = () => alive.current;
  const onSavedRef = useRef(onSaved);
  const apiRef = useRef(api);
  useEffect(() => { onSavedRef.current = onSaved; apiRef.current = api; });
  useEffect(() => {
    alive.current = true;
    const t = timers.current;
    return () => { alive.current = false; t.forEach(x => { clearTimeout(x); }); t.clear(); };
  }, []);

  // 編集できる項目の一覧 (案件を選んだら読む。読めなければ編集の入口を出さない)
  useEffect(() => {
    if (dealId === null) return;
    const ctl = new AbortController();
    void api.editSchema(dealId, ctl.signal).then(r => {
      if (ctl.signal.aborted) return;
      setSchemaState({ dealId, s: r.ok ? { phase: 'ready', schema: r.data } : { phase: 'error' } });
    });
    return () => { ctl.abort(); };
  }, [dealId, api]);
  const schema = useMemo<SchemaState>(() => (dealId === null ? { phase: 'idle' } : schemaState.dealId === dealId ? schemaState.s : { phase: 'loading' }), [dealId, schemaState]);
  const writesEnabled: boolean | null = schema.phase === 'ready' ? schema.schema.writes_enabled && !(dealId !== null && disabled.has(dealId)) : null;

  const setStatus = useCallback((keys: readonly string[], st: FieldStatus | null) => {
    setStatuses(prev => {
      const kept = Object.entries(prev).filter(([k]) => !keys.includes(k));
      return Object.fromEntries(st === null ? kept : [...kept, ...keys.map(k => [k, st] as const)]);
    });
  }, []);

  const keysOf = (req: SaveRequest): string[] => [
    ...req.changes.map(c => fieldKey(req.dealId, c.object, c.name)),
    ...(req.stage ? [stageKey(req.dealId)] : []),
  ];

  const setOverlays = useCallback((req: SaveRequest, saved: { values: PropValues; objects_values: Record<string, PropValues> } | null, pending: boolean) => {
    setOverlay(prev => {
      const next = { ...prev };
      for (const c of req.changes) {
        // 案件の項目は values、担当者・会社の項目は objects_values[contact|company] に保存後の値が入る
        const values = saved === null ? null : c.object === 'deal' ? saved.values : (saved.objects_values[c.object] ?? null);
        const v = values !== null && c.name in values ? values[c.name] ?? null : c.value;
        next[fieldKey(req.dealId, c.object, c.name)] = { value: v, pending, fetchedAt: req.fetchedAt ?? null };
      }
      return next;
    });
    if (req.stage) {
      const st = req.stage;
      setStageOverlay(prev => ({ ...prev, [req.dealId]: { pipeline_id: st.pipeline_id, stage_id: st.stage_id, label: st.label, pending, fetchedAt: req.fetchedAt ?? null } }));
    }
  }, []);

  const clearPending = useCallback((req: SaveRequest) => {
    const gone = new Set(req.changes.map(c => fieldKey(req.dealId, c.object, c.name)));
    setOverlay(prev => Object.fromEntries(Object.entries(prev).filter(([k]) => !gone.has(k))));
    if (req.stage) setStageOverlay(prev => Object.fromEntries(Object.entries(prev).filter(([k]) => k !== req.dealId)));
  }, []);

  const poll = useCallback((opId: string, req: SaveRequest) => {
    const maxPolls = Math.max(1, Math.ceil(POLL_MAX_MS / pollIntervalMs));
    let n = 0;
    const schedule = () => {
      const t = setTimeout(() => { timers.current.delete(t); void tick(); }, pollIntervalMs);
      timers.current.add(t);
    };
    const tick = async () => {
      if (!isAlive()) return;
      n += 1;
      const r = await apiRef.current.operation(opId);
      if (!isAlive()) return;
      if (r.ok && r.data.status === 'saved') {
        setStatus(keysOf(req), { phase: 'saved' });
        setOverlays(req, null, false);
        onSavedRef.current(req.dealId);
        return;
      }
      if (r.ok && r.data.status === 'failed') {
        setStatus(keysOf(req), { phase: 'error', message: 'HubSpot に反映できませんでした。管理者に確認を依頼してください。', req: null });
        clearPending(req);
        return;
      }
      if (n >= maxPolls) { setStatus(keysOf(req), { phase: 'queued', slow: true }); return; }
      schedule();
    };
    schedule();
  }, [pollIntervalMs, setStatus, setOverlays, clearPending]);

  const save = useCallback(async (req: SaveRequest): Promise<SaveOutcome> => {
    const sig = requestSignature(req);
    let opId = opIds.current.get(sig);
    if (opId === undefined) { opId = (newId ?? (() => crypto.randomUUID()))(); opIds.current.set(sig, opId); }
    const keys = keysOf(req);
    setStatus(keys, { phase: 'saving' });
    const out = await apiRef.current.patchDeal(req.dealId, buildPatchBody(req, opId));
    if (!isAlive()) return { kind: 'error', message: '' };
    switch (out.kind) {
      case 'saved':
        opIds.current.delete(sig);
        setOverlays(req, { values: out.response.values, objects_values: out.response.objects_values }, false);
        setStatus(keys, { phase: 'saved' });
        onSavedRef.current(req.dealId);
        return { kind: 'saved' };
      case 'queued':
        opIds.current.delete(sig);
        setOverlays(req, null, true);
        setStatus(keys, { phase: 'queued', slow: false });
        poll(out.response.operation_id, req);
        return { kind: 'queued' };
      case 'conflict':
        opIds.current.delete(sig);
        setStatus(keys, null);
        setConflict({ req, object: out.body.object, current: out.body.current, changed: out.body.changed_by_hubspot });
        return { kind: 'conflict' };
      case 'invalid': {
        opIds.current.delete(sig);
        const errs = out.body.errors;
        for (const c of req.changes) {
          const m = errs[c.name];
          setStatus([fieldKey(req.dealId, c.object, c.name)], m === undefined ? null : { phase: 'invalid', message: m });
        }
        if (req.stage) setStatus([stageKey(req.dealId)], { phase: 'invalid', message: '移すには必要な項目が足りません。' });
        return { kind: 'invalid', errors: errs, missing: out.body.missing_required };
      }
      case 'writes_disabled':
        opIds.current.delete(sig);
        setDisabled(prev => new Set(prev).add(req.dealId));
        setStatus(keys, null);
        return { kind: 'error', message: WRITES_OFF_NOTE };
      case 'forbidden':
        opIds.current.delete(sig);
        setStatus(keys, { phase: 'error', message: 'この操作をする権限がありません。', req: null });
        return { kind: 'error', message: 'この操作をする権限がありません。' };
      case 'queue_full': {
        const message = 'いま保存を受け付けられません。何も保存されていません。少し待ってからもう一度お試しください。';
        setStatus(keys, { phase: 'error', message, req });
        return { kind: 'error', message };
      }
      case 'error': {
        // HubSpot に届いたか分からない。同じ operation_id で再送できるよう鍵は残す
        const message = `${out.message}保存できたか確認できません。もう一度保存してください。`;
        setStatus(keys, { phase: 'error', message, req });
        return { kind: 'error', message };
      }
    }
  }, [newId, poll, setOverlays, setStatus]);

  const resolveConflict = useCallback((choice: 'theirs' | 'mine') => {
    const c = conflict;
    if (c === null) return;
    setConflict(null);
    if (choice === 'theirs') {
      setOverlay(prev => {
        const next = { ...prev };
        for (const ch of c.req.changes) if (ch.object === c.object && ch.name in c.current) next[fieldKey(c.req.dealId, ch.object, ch.name)] = { value: c.current[ch.name] ?? null, pending: false, fetchedAt: c.req.fetchedAt ?? null };
        return next;
      });
      onSavedRef.current(c.req.dealId);
      return;
    }
    // 自分の値で上書き: HubSpot の今の値を「見た値」として送り直す (新しい操作なので operation_id も新しくなる)
    void save({ ...c.req, changes: c.req.changes.map(ch => (ch.object === c.object && ch.name in c.current ? { ...ch, base: c.current[ch.name] ?? null } : ch)) });
  }, [conflict, save]);

  const dismiss = useCallback((key: string) => { setStatus([key], null); }, [setStatus]);

  return useMemo(() => ({
    schema, writesEnabled, statuses, overlay, stageOverlay, conflict, save, resolveConflict, dismiss,
  }), [schema, writesEnabled, statuses, overlay, stageOverlay, conflict, save, resolveConflict, dismiss]);
}

export type CrmWrite = ReturnType<typeof useCrmWrite>;
export type { WriteObject };
