import { useCallback, useEffect, useState } from 'react';
import { ApiHttpError, AuthRequiredError, apiGet } from '../../api/client';
import type { ApiResult } from '../../api/client';
import type { CrmMetadataResponse } from '../../generated/CrmMetadataResponse';
import { metadataDealDefinitions } from './liveMetadata';
import { MOC_DEAL_PROPERTIES } from './mocProperties';
import type { MocPropertyDefinition } from './mocProperties';
import { CLIENT_TIMEOUT_KIND, INVALID_RESPONSE_KIND, failureOf } from './queueModel';
import type { QueueMode } from './queueModel';
import { FALLBACK_LABELS, REQUIRED_DEFINITIONS, REQUIRED_TYPED_DEFINITIONS } from './callResultModel';

export type MetadataFetch = (signal: AbortSignal) => Promise<ApiResult<CrmMetadataResponse>>;
export const liveMetadataFetch: MetadataFetch = signal => apiGet<CrmMetadataResponse>('/api/crm/metadata', { signal, timeoutMs: 35_000 });

export type DefinitionsState =
  | { phase: 'loading' }
  | { phase: 'ready'; defs: Record<string, MocPropertyDefinition>; source: 'hubspot' | 'fixture' }
  | { phase: 'error'; message: string };

/**
 * 応答に入っていない (選択肢を出せない)、または型が想定と違う項目の内部名。
 * 選択肢のある項目は選択肢が 1 つ以上、日付・文字の項目はその型の定義があること
 */
export function missingDefinitions(defs: Record<string, MocPropertyDefinition>): string[] {
  const noOptions = REQUIRED_DEFINITIONS.filter(name => (defs[name]?.options.length ?? 0) === 0);
  const wrongType = Object.entries(REQUIRED_TYPED_DEFINITIONS).filter(([name, type]) => defs[name]?.type !== type).map(([name]) => name);
  return [...noOptions, ...wrongType];
}

/** 画面に出す項目名 (日本語だけ。内部名は出さない) */
const fieldLabel = (name: string) => MOC_DEAL_PROPERTIES[name]?.label ?? FALLBACK_LABELS[name] ?? '名称不明の項目';

/** 定義が足りないときの画面の文言。内部名は含めない (調べるときのためにコンソールへ出す) */
export function missingDefinitionsMessage(missing: readonly string[]): string {
  return `HubSpot から次の項目の選択肢・設定を受け取れませんでした: ${missing.map(fieldLabel).join('、')}`;
}

/** 選択肢を読めなかったときの文言。原因に合わせて、利用者ができること (待つ・再試行・管理者に連絡) を添える */
export function metadataErrorMessage(error: unknown): string {
  if (error instanceof AuthRequiredError || (error instanceof ApiHttpError && error.status === 401)) return 'ログインが切れています。再読み込みしてログインしてください。';
  if (error instanceof ApiHttpError && error.status === 403) return 'このアカウントでは HubSpot の選択肢を読み込めません。管理者に連絡してください。';
  const { kind, status } = failureOf(error);
  const head = 'HubSpot から選択肢を読み込めませんでした。';
  switch (kind) {
    case 'hubspot_rate_limited': return `${head}HubSpot の呼び出し回数の上限に達しています。少し待ってから再試行してください。`;
    case 'hubspot_busy': return `${head}HubSpot が混み合っています。少し待ってから再試行してください。`;
    case 'hubspot_timeout':
    case 'crm_timeout':
    case CLIENT_TIMEOUT_KIND: return `${head}応答が時間内に返りませんでした。少し待ってから再試行してください。`;
    case 'hubspot_auth':
    case 'not_configured':
    case 'hubspot_decode':
    case INVALID_RESPONSE_KIND: return `${head}接続の設定に問題がある可能性があります。管理者に連絡してください。`;
    case 'hubspot_upstream':
    case 'hubspot_transport': return `${head}HubSpot との通信に失敗しました。再試行してください。`;
    default: return status === null
      ? `${head}ネットワークに接続できませんでした。接続を確認して再試行してください。`
      : `${head}(${String(status)})再試行してください。`;
  }
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
      if (!r.ok) { setLive({ phase: 'error', message: metadataErrorMessage(r.error) }); return; }
      const defs = metadataDealDefinitions(r.data);
      const missing = missingDefinitions(defs);
      if (missing.length > 0) {
        console.warn('[crm] /api/crm/metadata lacks required deal definitions:', missing.join(', '));
        setLive({ phase: 'error', message: missingDefinitionsMessage(missing) });
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
