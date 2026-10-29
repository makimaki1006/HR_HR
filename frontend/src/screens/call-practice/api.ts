// 架電の練習の API 呼び出し。本物 (サーバ) と、決まった返事で動く架空サンプル (?mode=fixture) を同じ形で持つ。

import { ApiHttpError, ApiError, AuthRequiredError, apiGet, apiPost, type ApiResult } from '../../api/client';
import type { PracticeConfig } from '../../generated/PracticeConfig';
import type { PracticeFinishRequest } from '../../generated/PracticeFinishRequest';
import type { PracticeResult } from '../../generated/PracticeResult';
import type { PracticeSessionView } from '../../generated/PracticeSessionView';
import type { PracticeStartRequest } from '../../generated/PracticeStartRequest';
import type { PracticeStartResponse } from '../../generated/PracticeStartResponse';
import type { PracticeTurnRequest } from '../../generated/PracticeTurnRequest';
import type { PracticeTurnResponse } from '../../generated/PracticeTurnResponse';
import { parseSse } from './sse';

export const GENERIC_ERROR = 'うまく動きませんでした。もう一度試してください。';

/** 画面にそのまま出せる日本語の message を持つ失敗 */
export class PracticeError extends Error {
  readonly kind: string;
  constructor(kind: string, message: string) {
    super(message);
    this.name = 'PracticeError';
    this.kind = kind;
  }
}

export interface PracticeApi {
  config: () => Promise<PracticeConfig>;
  start: (req: PracticeStartRequest) => Promise<PracticeStartResponse>;
  get: (id: string) => Promise<PracticeSessionView>;
  turn: (id: string, req: PracticeTurnRequest, onDelta: (text: string) => void) => Promise<PracticeTurnResponse>;
  finish: (id: string, req: PracticeFinishRequest) => Promise<PracticeResult>;
}

function bodyError(body: unknown): PracticeError | null {
  if (typeof body !== 'object' || body === null) return null;
  const b = body as { error_kind?: unknown; message?: unknown };
  if (typeof b.error_kind !== 'string') return null;
  return new PracticeError(b.error_kind, typeof b.message === 'string' ? b.message : GENERIC_ERROR);
}

function toPracticeError(e: ApiError): PracticeError {
  if (e instanceof AuthRequiredError) return new PracticeError('auth', 'ログインし直してください。');
  if (e instanceof ApiHttpError) return bodyError(e.body) ?? new PracticeError('http', GENERIC_ERROR);
  return new PracticeError('network', GENERIC_ERROR);
}

function unwrap<T>(r: ApiResult<T>): T {
  if (r.ok) return r.data;
  throw toPracticeError(r.error);
}

export const realApi: PracticeApi = {
  config: async () => unwrap(await apiGet<PracticeConfig>('/api/call-practice/config')),
  start: async (req) => unwrap(await apiPost<PracticeStartResponse>('/api/call-practice/sessions', req)),
  get: async (id) => unwrap(await apiGet<PracticeSessionView>(`/api/call-practice/sessions/${encodeURIComponent(id)}`)),
  finish: async (id, req) =>
    unwrap(
      await apiPost<PracticeResult>(`/api/call-practice/sessions/${encodeURIComponent(id)}/finish`, req, {
        timeoutMs: 120_000,
      }),
    ),
  turn: async (id, req, onDelta) => {
    let res: Response;
    try {
      res = await fetch(`/api/call-practice/sessions/${encodeURIComponent(id)}/turns`, {
        method: 'POST',
        credentials: 'same-origin',
        headers: { 'Content-Type': 'application/json', Accept: 'text/event-stream', 'X-Requested-With': 'fetch' },
        body: JSON.stringify(req),
      });
    } catch {
      throw new PracticeError('network', GENERIC_ERROR);
    }
    if (!res.ok) {
      let body: unknown = null;
      try {
        body = await res.json();
      } catch {
        // JSON でない失敗
      }
      throw bodyError(body) ?? new PracticeError('http', GENERIC_ERROR);
    }
    if (res.body === null) throw new PracticeError('network', GENERIC_ERROR);
    const reader = res.body.getReader();
    const decoder = new TextDecoder();
    let buf = '';
    let done: PracticeTurnResponse | null = null;
    try {
      for (;;) {
        const { value, done: finished } = await reader.read();
        if (value !== undefined) buf += decoder.decode(value, { stream: !finished });
        const parsed = parseSse(buf);
        buf = parsed.rest;
        for (const ev of parsed.events) {
          if (ev.event === 'delta') {
            const t = (ev.data as { text?: unknown }).text;
            if (typeof t === 'string') onDelta(t);
          } else if (ev.event === 'done') {
            done = ev.data as PracticeTurnResponse;
          } else if (ev.event === 'error') {
            throw bodyError(ev.data) ?? new PracticeError('llm_busy', GENERIC_ERROR);
          }
        }
        if (finished) break;
      }
    } catch (e) {
      if (e instanceof PracticeError) throw e;
      throw new PracticeError('network', GENERIC_ERROR);
    }
    if (done === null) throw new PracticeError('network', GENERIC_ERROR);
    return done;
  },
};
