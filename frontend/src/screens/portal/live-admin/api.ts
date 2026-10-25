// 応募者画面の社内の管理 (/app/portal-admin) のサーバーとのやり取り。/api/admin/portal/* (管理者だけ) を呼ぶ。
// 社内の API クライアント (X-Requested-With: fetch を付ける) を使い、未ログインならログインへ移る。
import { ApiAbortedError, ApiHttpError, AuthRequiredError, apiGet, apiPost, apiPut, type ApiError, type ApiResult } from '../../../api/client';
import { redirectToLogin } from '../../../shell/navigation';

export type Result<T> = { ok: true; data: T } | { ok: false; status: number; error: string; message: string; extra: Record<string, unknown> };

/** 失敗をそのまま画面に出せる文にする。サーバーの文があればそれを使う */
export function toResult<T>(r: ApiResult<T>): Result<T> {
  if (r.ok) return r;
  return fromError(r.error);
}

export function fromError(e: ApiError): { ok: false; status: number; error: string; message: string; extra: Record<string, unknown> } {
  if (e instanceof AuthRequiredError) {
    return { ok: false, status: 401, error: 'unauthorized', message: 'ログインし直してから開いてください。', extra: {} };
  }
  if (e instanceof ApiHttpError) {
    const body = (typeof e.body === 'object' && e.body !== null ? e.body : {}) as Record<string, unknown>;
    const message = typeof body.message === 'string' && body.message !== '' ? body.message : defaultMessage(e.status);
    return { ok: false, status: e.status, error: typeof body.error === 'string' ? body.error : 'unknown', message, extra: body };
  }
  if (e instanceof ApiAbortedError) return { ok: false, status: 0, error: 'aborted', message: '', extra: {} };
  return { ok: false, status: 0, error: 'network', message: 'サーバーにつながりません。少し待ってからやり直してください。', extra: {} };
}

function defaultMessage(status: number): string {
  if (status === 403) return 'この画面は管理者だけが使えます。';
  if (status === 404) return '見つかりません。';
  if (status === 503) return 'いまは使えません。少し待ってから開き直してください。';
  return 'うまくいきませんでした。少し待ってからやり直してください。';
}

function handleAuth<T>(r: Result<T>): Result<T> {
  if (!r.ok && r.status === 401) redirectToLogin();
  return r;
}

export async function get<T>(path: string, signal?: AbortSignal): Promise<Result<T>> {
  return handleAuth(toResult(await apiGet<T>(path, signal ? { signal } : {})));
}

export async function post<T>(path: string, body: unknown): Promise<Result<T>> {
  return handleAuth(toResult(await apiPost<T>(path, body)));
}

export async function put<T>(path: string, body: unknown): Promise<Result<T>> {
  return handleAuth(toResult(await apiPut<T>(path, body)));
}

export const API = '/api/admin/portal';
