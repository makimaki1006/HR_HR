// Placeholder screen used only to prove the build / manifest / mount path (Phase 0).
// Replaced by real screens (recruitment_diag first) in Phase 1A.
//
// Phase 0-4: calls GET /api/app/ping through the shared API client. The response
// type is generated from the Rust struct by ts-rs (src/generated/), so a field
// rename on the Rust side breaks `npm run typecheck` here.
import { useEffect, useState } from 'react';
import { ApiAbortedError, apiGet } from '../../api/client';
import type { AppPingResponse } from '../../generated/AppPingResponse';

export const PING_PATH = '/api/app/ping';

export type PingState =
  | { status: 'loading' }
  | { status: 'ok'; data: AppPingResponse }
  | { status: 'error'; message: string };

export function PingView({ state }: { state: PingState }) {
  switch (state.status) {
    case 'loading':
      return <p data-testid="ping-loading">サーバに問い合わせ中…</p>;
    case 'error':
      return (
        <p role="alert" data-testid="ping-error">
          サーバ応答を取得できませんでした: {state.message}
        </p>
      );
    case 'ok':
      return (
        <dl data-testid="ping-result">
          <dt>message</dt>
          <dd data-testid="ping-message">{state.data.message}</dd>
          <dt>server_time (UTC)</dt>
          <dd data-testid="ping-server-time">{state.data.server_time}</dd>
        </dl>
      );
  }
}

export function DummyScreen() {
  const [state, setState] = useState<PingState>({ status: 'loading' });

  useEffect(() => {
    const controller = new AbortController();
    void apiGet<AppPingResponse>(PING_PATH, { signal: controller.signal }).then((result) => {
      if (result.ok) {
        setState({ status: 'ok', data: result.data });
      } else if (!(result.error instanceof ApiAbortedError)) {
        setState({ status: 'error', message: result.error.message });
      }
    });
    return () => {
      controller.abort();
    };
  }, []);

  return (
    <main>
      <h1>React 基盤の疎通確認画面</h1>
      <p>この画面は Phase 0 の動作確認用です。業務データは表示しません。</p>
      <PingView state={state} />
    </main>
  );
}
