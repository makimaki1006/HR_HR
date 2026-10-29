// /app/admin?view=calendar: 応募の日付を載せる管理用カレンダー (ADR-037)。管理者だけ。
// 許可を押せるのは管理用カレンダーのアカウントの本人だけ (サーバーが確かめる)。取り消しは管理者なら誰でも。
// データは GET /api/admin/applicant-calendar。取り消しは POST /api/admin/applicant-calendar/revoke。
import { useState } from 'react';
import { apiPost } from '../../api/client';
import type { ApplicantCalendarStatus } from '../../generated/ApplicantCalendarStatus';
import { describeApiError, useApiGet } from './useApiGet';

const STATE_LABEL: Record<string, string> = {
  granted: '許可済み',
  none: '未許可',
  expired: '切れています',
  unknown: '確かめられません',
};

/** ISO 8601 (UTC) を日本時間の「YYYY/MM/DD HH:MM」に */
export function jstText(iso: string | null): string {
  if (iso === null) return '';
  const t = new Date(iso);
  if (Number.isNaN(t.getTime())) return '';
  const j = new Date(t.getTime() + 9 * 3600 * 1000);
  const p = (n: number) => String(n).padStart(2, '0');
  return `${String(j.getUTCFullYear())}/${p(j.getUTCMonth() + 1)}/${p(j.getUTCDate())} ${p(j.getUTCHours())}:${p(j.getUTCMinutes())}`;
}

export function CalendarView({ data, onChanged }: { data: ApplicantCalendarStatus; onChanged: () => void }) {
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState('');
  const kinds = data.kinds.join('・');
  const heading = (
    <>
      <h1 className="w8-h1" style={{ marginBottom: 4 }}>応募の日付のカレンダー</h1>
      <p className="w8-subtle">
        HubSpot の応募の{kinds}の日付を、管理用カレンダーに予定として載せます。予定にはその応募のコンサル担当をゲストに入れます（招待のメールは送りません）。
      </p>
    </>
  );
  if (!data.configured) {
    return (
      <>
        {heading}
        <p className="w8-alert" data-testid="calendar-not-configured">
          <strong>設定がされていません。</strong> 管理用カレンダーのアカウントなどの設定がそろうと使えるようになります（設定は開発の担当者に頼んでください）。
        </p>
      </>
    );
  }
  const revoke = async () => {
    setBusy(true);
    setMessage('');
    const r = await apiPost<{ state: string; google_revocation_pending: boolean }>('/api/admin/applicant-calendar/revoke', {});
    setBusy(false);
    if (!r.ok) {
      setMessage('取り消せませんでした。少し待ってからもう一度お試しください。');
      return;
    }
    if (r.data.google_revocation_pending) {
      setMessage('この画面では取り消しました。Google 側の取り消しはできなかったので、管理用カレンダーのアカウントの Google の設定からも取り消してください。');
    }
    onChanged();
  };
  const granted = data.state === 'granted';
  return (
    <>
      {heading}
      {data.state === 'expired' && (
        <p className="w8-alert w8-alert-red" role="alert" data-testid="calendar-expired">
          <strong>許可が切れています。</strong> カレンダーに載せられません。管理用カレンダーのアカウントの人がこの画面を開き、もう一度許可してください。
        </p>
      )}
      {data.state === 'none' && (
        <p className="w8-alert" data-testid="calendar-none">
          <strong>まだ許可されていません。</strong> 許可されるまで、カレンダーに載せません。
        </p>
      )}
      {!data.enabled && (
        <p className="w8-note" data-testid="calendar-disabled">いまは自動で載せる設定が止まっています。許可は先に済ませておけます。</p>
      )}
      <div className="w8-card">
        <table className="w8-table w8-table-dense">
          <tbody>
            <tr><th>管理用カレンダー</th><td>{data.account}</td></tr>
            <tr>
              <th>許可</th>
              <td data-testid="calendar-state">
                {STATE_LABEL[data.state] ?? '確かめられません'}
                {granted && data.granted_at !== null && <span className="w8-subtle">（{jstText(data.granted_at)} に許可）</span>}
              </td>
            </tr>
            <tr>
              <th>最後に同期した時刻</th>
              <td data-testid="calendar-last-sync">{data.last_sync_at === null ? 'まだありません' : jstText(data.last_sync_at)}</td>
            </tr>
            <tr>
              <th>直近の同期</th>
              <td data-testid="calendar-last-counts">
                追加 {data.last_created} 件・変更 {data.last_updated} 件・削除 {data.last_deleted} 件・書けなかった {data.last_errors} 件
              </td>
            </tr>
            <tr>
              <th>書けなかった件数</th>
              <td data-testid="calendar-errors">{data.errors_since_start} 件（アプリを起動してからの合計）</td>
            </tr>
          </tbody>
        </table>
      </div>
      {data.problem !== null && <p className="w8-alert" role="status">{data.problem}</p>}
      <p style={{ marginTop: 12 }}>
        {data.is_account ? (
          <a className="w8-button" href="/api/admin/applicant-calendar/authorize">
            {granted ? '許可し直す' : '管理用カレンダーへの書き込みを許可する'}
          </a>
        ) : (
          <span className="w8-subtle">許可は管理用カレンダーのアカウントの人だけが押せます。その人にこの画面を開いてもらってください。</span>
        )}{' '}
        {(granted || data.state === 'expired') && (
          <button type="button" className="w8-button" disabled={busy} onClick={() => { void revoke(); }}>
            許可を取り消す
          </button>
        )}
      </p>
      {message !== '' && <p className="w8-alert" role="status">{message}</p>}
      <p className="w8-note">
        応募の日付を変えると、数分後にカレンダーの予定も移ります。日付を消すと予定も消えます。予定はカレンダーで直さず、HubSpot の応募で直してください。
      </p>
    </>
  );
}

export function CalendarPanel() {
  const [round, setRound] = useState(0);
  const state = useApiGet<ApplicantCalendarStatus>(`/api/admin/applicant-calendar${round === 0 ? '' : `?r=${String(round)}`}`);
  if (state.status === 'loading') return <p>読み込み中…</p>;
  if (state.status === 'error') {
    const t = describeApiError(state.error);
    return <p className="w8-alert" role="alert"><strong>{t.title}</strong> {t.detail}</p>;
  }
  return <CalendarView data={state.data} onChanged={() => { setRound(n => n + 1); }} />;
}
