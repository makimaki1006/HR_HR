// 顧客向け応募者画面 (/portal/*) のサーバーとのやり取り。社内の API クライアントは使わない。
export type ApiResult<T> =
  | { ok: true; data: T }
  | { ok: false; status: number; error: string; message: string; extra: Record<string, unknown> };

export const NETWORK = 'network';

export async function call<T>(method: 'GET' | 'POST', path: string, body?: unknown): Promise<ApiResult<T>> {
  let res: Response;
  try {
    const init: RequestInit = {
      method,
      credentials: 'same-origin',
      headers: { 'X-Requested-With': 'fetch', Accept: 'application/json', 'Content-Type': 'application/json' },
    };
    if (body !== undefined) init.body = JSON.stringify(body);
    res = await fetch(path, init);
  } catch {
    return { ok: false, status: 0, error: NETWORK, message: '', extra: {} };
  }
  const json: unknown = await res.json().catch(() => null);
  if (res.ok) return { ok: true, data: json as T };
  const o = (json ?? {}) as Record<string, unknown>;
  return {
    ok: false,
    status: res.status,
    error: typeof o.error === 'string' ? o.error : 'unknown',
    message: typeof o.message === 'string' ? o.message : '',
    extra: o,
  };
}

/** 招待のリンクの値。#t=... から読む (サーバーのログに残さないため # の後ろに置いている) */
export function inviteTokenFromHash(hash: string): string {
  const m = /(?:^#|&)t=([A-Za-z0-9_-]+)/.exec(hash);
  return m?.[1] ?? '';
}
