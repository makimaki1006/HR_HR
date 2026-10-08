import { describe, expect, it } from 'vitest';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { ApiHttpError, ApiNetworkError, ApiTimeoutError, AuthRequiredError } from '../../api/client';
import { HUBSPOT_BUSY_MESSAGE, isHubSpotBusy, snapshotErrorGuidance, SnapshotErrorNotice } from './SnapshotErrorNotice';

describe('snapshot error user guidance', () => {
  it('asks to wait and retry when the server says HubSpot is busy (503 hubspot_busy)', () => {
    const busy = new ApiHttpError(503, { code: 'hubspot_busy' });
    expect(isHubSpotBusy(busy)).toBe(true);
    expect(snapshotErrorGuidance(busy)).toEqual({ message: 'HubSpot が混み合っています。少し待ってから再試行してください。' });
    expect(HUBSPOT_BUSY_MESSAGE).toBe('HubSpot が混み合っています。少し待ってから再試行してください。');
    // Other 503s and other codes are not "busy"
    expect(isHubSpotBusy(new ApiHttpError(503, { code: 'drive_not_configured' }))).toBe(false);
    expect(isHubSpotBusy(new ApiHttpError(502, { code: 'hubspot_busy' }))).toBe(false);
    expect(isHubSpotBusy(new ApiNetworkError('offline'))).toBe(false);
  });
  it.each([new ApiHttpError(401, { code: 'login_required' }), new AuthRequiredError('redirected to login')])('links authentication failures to the existing login page', error => {
    const guidance = snapshotErrorGuidance(error);
    expect(guidance.message).toContain('再ログイン');
    expect(renderToStaticMarkup(createElement(SnapshotErrorNotice, { guidance }))).toContain('href="/login"');
  });
  it('offers account switching and an administrator permission request for a 403', () => {
    const guidance = snapshotErrorGuidance(new ApiHttpError(403, { code: 'job_copy_access_denied' }));
    expect(guidance.message).toContain('閲覧する権限がありません');
    expect(guidance.message).toContain('Google Workspace');
    expect(guidance.login).toBe(true);
  });
  it('does not imply reauthentication can enable a disabled account', () => {
    const guidance = snapshotErrorGuidance(new ApiHttpError(403, { code: 'account_disabled' }));
    expect(guidance.message).toContain('無効');
    expect(guidance.login).toBeUndefined();
  });
  it.each([new ApiHttpError(404, { code: 'moc_not_configured' }), new ApiHttpError(503, { code: 'drive_not_configured' }), new ApiHttpError(503, { code: 'moc_drive_configuration_invalid' })])('directs missing server configuration to the administrator instead of login', error => {
    const guidance = snapshotErrorGuidance(error);
    expect(guidance.message).toContain('管理者');
    expect(guidance.message).toContain('設定');
    expect(guidance.login).toBeUndefined();
  });
  it('explains unreadable remote review data without displaying its raw code', () => {
    const guidance = snapshotErrorGuidance(new ApiHttpError(502, { code: 'moc_drive_snapshot_unavailable' }));
    expect(guidance.message).toContain('Drive上のレビュー用データを読み取れませんでした');
    expect(guidance.message).toContain('管理者');
    expect(guidance.message).not.toContain('moc_drive_snapshot_unavailable');
    expect(guidance.login).toBeUndefined();
  });
  it('directs invalid listing coverage configuration to the administrator without exposing its raw code', () => {
    const guidance = snapshotErrorGuidance(new ApiHttpError(503, { code: 'drive_listing_configuration_invalid' }));
    expect(guidance.message).toContain('連携設定が不足');
    expect(guidance.message).toContain('管理者');
    expect(guidance.message).not.toContain('drive_listing_configuration_invalid');
    expect(guidance.login).toBeUndefined();
  });
  it('offers reload for timeouts and never renders raw network error details', () => {
    expect(snapshotErrorGuidance(new ApiTimeoutError(30000)).message).toContain('時間がかかっています');
    const guidance = snapshotErrorGuidance(new ApiNetworkError('PRIVATE_REMOTE_DETAIL'));
    expect(guidance.message).toContain('再読み込み');
    expect(renderToStaticMarkup(createElement(SnapshotErrorNotice, { guidance }))).not.toContain('PRIVATE_REMOTE_DETAIL');
  });
});
