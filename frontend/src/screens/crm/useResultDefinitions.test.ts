import { describe, expect, it } from 'vitest';
import { ApiHttpError, ApiInvalidResponseError, ApiNetworkError, ApiTimeoutError, AuthRequiredError } from '../../api/client';
import { metadataErrorMessage } from './useResultDefinitions';

const HEAD = 'HubSpot から選択肢を読み込めませんでした。';

describe('metadataErrorMessage', () => {
  it.each([
    ['429 rate limit', new ApiHttpError(503, { error_kind: 'hubspot_rate_limited' }), `${HEAD}HubSpot の呼び出し回数の上限に達しています。少し待ってから再試行してください。`],
    ['gateway busy', new ApiHttpError(503, { error_kind: 'hubspot_busy' }), `${HEAD}HubSpot が混み合っています。少し待ってから再試行してください。`],
    ['HubSpot timeout', new ApiHttpError(504, { error_kind: 'hubspot_timeout' }), `${HEAD}応答が時間内に返りませんでした。少し待ってから再試行してください。`],
    ['client timeout', new ApiTimeoutError(35_000), `${HEAD}応答が時間内に返りませんでした。少し待ってから再試行してください。`],
    ['auth config', new ApiHttpError(502, { error_kind: 'hubspot_auth' }), `${HEAD}接続の設定に問題がある可能性があります。管理者に連絡してください。`],
    ['unreadable', new ApiInvalidResponseError('bad'), `${HEAD}接続の設定に問題がある可能性があります。管理者に連絡してください。`],
    ['upstream', new ApiHttpError(502, { error_kind: 'hubspot_upstream' }), `${HEAD}HubSpot との通信に失敗しました。再試行してください。`],
    ['network', new ApiNetworkError('offline'), `${HEAD}ネットワークに接続できませんでした。接続を確認して再試行してください。`],
    ['unknown status', new ApiHttpError(500), `${HEAD}(500)再試行してください。`],
    ['login expired', new AuthRequiredError('x'), 'ログインが切れています。再読み込みしてログインしてください。'],
    ['forbidden', new ApiHttpError(403, { error_kind: 'forbidden' }), 'このアカウントでは HubSpot の選択肢を読み込めません。管理者に連絡してください。'],
  ])('%s', (_name, error, want) => {
    expect(metadataErrorMessage(error)).toBe(want);
  });

  it('never asks the caller to enter values in HubSpot directly (callers have no HubSpot seat)', () => {
    for (const e of [new ApiHttpError(500), new ApiNetworkError('x'), new ApiTimeoutError(1)]) expect(metadataErrorMessage(e)).not.toContain('直接');
  });
});
