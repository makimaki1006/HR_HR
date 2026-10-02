import { useEffect, useState } from 'react';
import { ApiAbortedError, AuthRequiredError, apiGet } from '../../api/client';
import { redirectToLogin } from '../../shell/navigation';
import type { GuideResponse } from '../../generated/GuideResponse';
import { GuideView } from './GuideView';
import './guide.css';

export const GUIDE_PATH = '/api/guide';

type State =
  | { status: 'loading' }
  | { status: 'ok'; data: GuideResponse }
  | { status: 'error'; message: string };

export function GuideScreen() {
  const [state, setState] = useState<State>({ status: 'loading' });

  useEffect(() => {
    const controller = new AbortController();
    void apiGet<GuideResponse>(GUIDE_PATH, { signal: controller.signal }).then((result) => {
      // A response that arrives after the request was aborted must not overwrite newer state.
      if (controller.signal.aborted) return;
      if (result.ok) {
        setState({ status: 'ok', data: result.data });
      } else if (result.error instanceof AuthRequiredError) {
        redirectToLogin();
      } else if (!(result.error instanceof ApiAbortedError)) {
        setState({ status: 'error', message: result.error.message });
      }
    });
    return () => {
      controller.abort();
    };
  }, []);

  return (
    <main className="guide">
      {state.status === 'loading' && <p className="guide__status">読み込み中…</p>}
      {state.status === 'error' && (
        <p className="guide__status" role="alert">
          ガイドを取得できませんでした: {state.message}
        </p>
      )}
      {state.status === 'ok' && <GuideView data={state.data} />}
    </main>
  );
}
