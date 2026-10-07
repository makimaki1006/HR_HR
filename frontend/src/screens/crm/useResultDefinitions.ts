import { useCallback, useEffect, useState } from 'react';
import { ApiHttpError, AuthRequiredError, apiGet } from '../../api/client';
import type { ApiResult } from '../../api/client';
import type { CrmMetadataResponse } from '../../generated/CrmMetadataResponse';
import { metadataDealDefinitions } from './liveMetadata';
import { MOC_DEAL_PROPERTIES } from './mocProperties';
import type { MocPropertyDefinition } from './mocProperties';
import type { QueueMode } from './queueModel';
import { FIELD_PROPERTY, REQUIRED_DEFINITIONS } from './callResultModel';

export type MetadataFetch = (signal: AbortSignal) => Promise<ApiResult<CrmMetadataResponse>>;
export const liveMetadataFetch: MetadataFetch = signal => apiGet<CrmMetadataResponse>('/api/crm/metadata', { signal, timeoutMs: 35_000 });

export type DefinitionsState =
  | { phase: 'loading' }
  | { phase: 'ready'; defs: Record<string, MocPropertyDefinition>; source: 'hubspot' | 'fixture' }
  | { phase: 'error'; message: string };

/** 応答に入っていない (選択肢を出せない) 項目の内部名 */
export function missingDefinitions(defs: Record<string, MocPropertyDefinition>): string[] {
  return REQUIRED_DEFINITIONS.filter(name => (defs[name]?.options.length ?? 0) === 0);
}

const LABELS: Record<string, string> = Object.fromEntries(
  Object.values(FIELD_PROPERTY).map(n => [n, MOC_DEAL_PROPERTIES[n]?.label ?? n]),
);

function errorMessage(error: unknown): string {
  if (error instanceof AuthRequiredError || (error instanceof ApiHttpError && error.status === 401)) return 'ログインが切れています。再読み込みしてログインしてください。';
  if (error instanceof ApiHttpError && error.status === 403) return 'このアカウントには HubSpot の項目定義を読む権限がありません。';
  return 'HubSpot から選択肢の定義を取得できませんでした。';
}

/**
 * 架電結果の入力欄の選択肢。
 * - 架空サンプル: mocProperties.ts の固定定義 (HubSpot には接続しない)
 * - 実データ: GET /api/crm/metadata。失敗したら error (固定定義で代用しない)。必要な項目が応答に無いときも error
 * `enabled` が false の間は取得しない (案件を選ぶまで HubSpot を呼ばない)
 */
export function useResultDefinitions(mode: QueueMode, enabled: boolean, fetcher: MetadataFetch = liveMetadataFetch): { state: DefinitionsState; reload: () => void } {
  const [live, setLive] = useState<DefinitionsState>({ phase: 'loading' });
  const [attempt, setAttempt] = useState(0);
  const needLive = mode === 'live' && enabled && live.phase === 'loading';

  useEffect(() => {
    if (!needLive) return;
    const ctl = new AbortController();
    void fetcher(ctl.signal).then(r => {
      if (ctl.signal.aborted) return;
      if (!r.ok) { setLive({ phase: 'error', message: errorMessage(r.error) }); return; }
      const defs = metadataDealDefinitions(r.data);
      const missing = missingDefinitions(defs);
      if (missing.length > 0) {
        setLive({ phase: 'error', message: `HubSpot の応答に次の項目の選択肢がありません: ${missing.map(n => `${LABELS[n] ?? n}(${n})`).join('、')}` });
        return;
      }
      setLive({ phase: 'ready', defs, source: 'hubspot' });
    });
    return () => { ctl.abort(); };
  }, [needLive, fetcher, attempt]);

  const reload = useCallback(() => { setLive({ phase: 'loading' }); setAttempt(a => a + 1); }, []);
  if (mode === 'fixture') return { state: { phase: 'ready', defs: MOC_DEAL_PROPERTIES, source: 'fixture' }, reload };
  return { state: live, reload };
}
