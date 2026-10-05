import { ApiHttpError, ApiTimeoutError, AuthRequiredError } from '../../api/client';
import type { ApiError } from '../../api/client';

export interface SnapshotErrorGuidance { message: string; login?: boolean }

function errorCode(error: ApiError): string | undefined {
  const body = error instanceof ApiHttpError ? error.body : undefined;
  return typeof body === 'object' && body !== null && !Array.isArray(body) && 'code' in body
    && typeof body.code === 'string' && /^[a-z][a-z0-9_]{0,79}$/.test(body.code) ? body.code : undefined;
}

/** User guidance only: Rust remains the authentication/authorization boundary. */
export function snapshotErrorGuidance(error: ApiError): SnapshotErrorGuidance {
  const code = errorCode(error);
  if (error instanceof AuthRequiredError || (error instanceof ApiHttpError && error.status === 401)) {
    return { message: 'ログインが必要です。セッションが切れている場合は、再ログインして求人文面画面を開き直してください。', login: true };
  }
  if (error instanceof ApiHttpError && error.status === 403) {
    if (code === 'account_disabled') return { message: 'このアカウントは無効になっています。管理者にアカウントの状態を確認してください。' };
    return { message: '求人文面を閲覧する権限がありません。Google Workspaceの社内アカウントでログインしているか確認し、管理者に閲覧権限を依頼してください。別のアカウントで再ログインできます。', login: true };
  }
  if (error instanceof ApiHttpError && (code === 'moc_not_configured' || error.status === 404)) {
    return { message: '求人文面の実データがサーバーに設定されていません。管理者にデータの設定を依頼してください。' };
  }
  if (error instanceof ApiHttpError && (code?.endsWith('_not_configured') || code === 'moc_drive_configuration_invalid' || code === 'drive_listing_configuration_invalid')) {
    return { message: '実データまたは画像の連携設定が不足しています。管理者にサーバーの設定を確認してもらってください。' };
  }
  if (error instanceof ApiHttpError && code === 'moc_drive_snapshot_unavailable') {
    return { message: 'Drive上のレビュー用データを読み取れませんでした。管理者にデータの配置と閲覧権限を確認してもらってください。' };
  }
  if (error instanceof ApiTimeoutError) return { message: '実データの取得に時間がかかっています。少し待ってから画面を再読み込みしてください。続く場合は管理者に取得状況を確認してください。' };
  return { message: '実データMOCを取得できませんでした。画面を再読み込みしてください。続く場合は管理者にデータと画像の取得状況を確認してください。' };
}

export function SnapshotErrorNotice({ guidance }: { guidance: SnapshotErrorGuidance }) {
  return <div className="jc-error" role="alert"><p>{guidance.message}</p>{guidance.login && <a className="jc-button" href="/login">再ログインする</a>}</div>;
}
