// /app/admin?view=crm-failed: HubSpot に保存できなかった操作の一覧 (管理者だけ)。
// データは GET /api/admin/crm-operations?status=failed。再試行・破棄は POST /api/admin/crm-operations/{id}/retry|discard。
import { useState } from 'react';
import { apiPost } from '../../api/client';
import type { CrmAdminOperation } from '../../generated/CrmAdminOperation';
import type { CrmAdminOperationsResponse } from '../../generated/CrmAdminOperationsResponse';
import { describeApiError, useApiGet } from './useApiGet';

/** 画面に出す失敗の理由 (内部のコードは出さない) */
export function failureReason(code: string | null): string {
  switch (code) {
    case 'conflict':
      return '保存しようとしたあとで、HubSpot の値が変わっていました';
    case 'missing_required':
      return '必須の項目が足りませんでした';
    case 'max_attempts':
      return '何度か試しましたが保存できませんでした';
    default:
      return 'HubSpot が受け付けませんでした';
  }
}

export function FailedOpsView({ rows, onChanged }: { rows: CrmAdminOperation[]; onChanged: (id: string) => void }) {
  const [busy, setBusy] = useState<string | null>(null);
  const [message, setMessage] = useState('');
  const act = async (id: string, kind: 'retry' | 'discard') => {
    setBusy(id);
    setMessage('');
    const r = await apiPost<unknown>(`/api/admin/crm-operations/${encodeURIComponent(id)}/${kind}`, {});
    setBusy(null);
    if (r.ok) onChanged(id);
    else setMessage('操作できませんでした。少し待ってからもう一度お試しください。');
  };
  return (
    <>
      <h1 className="w8-h1" style={{ marginBottom: 4 }}>HubSpot に保存できなかった操作</h1>
      <p className="w8-subtle">入力された内容を HubSpot に保存できなかったものです。「もう一度保存する」で再送し、不要なものは「破棄する」で一覧から外せます。</p>
      {message !== '' && <p className="w8-alert" role="status">{message}</p>}
      {rows.length === 0 ? (
        <p data-testid="failed-ops-empty">保存できなかった操作はありません。</p>
      ) : (
        <div className="w8-table-wrap">
          <table className="w8-table w8-table-dense" data-testid="failed-ops-table">
            <thead><tr><th>日時</th><th>操作した人</th><th>取引</th><th>変えようとした項目</th><th>理由</th><th /></tr></thead>
            <tbody>
              {rows.map(r => (
                <tr key={r.operation_id}>
                  <td>{r.updated_at.slice(0, 16).replace('T', ' ')}</td>
                  <td>{r.operator_email}</td>
                  <td>{r.deal_id}</td>
                  <td>{r.props.join('、')}</td>
                  <td>{failureReason(r.error)}</td>
                  <td>
                    <button type="button" className="w8-button" disabled={busy !== null} onClick={() => { void act(r.operation_id, 'retry'); }}>もう一度保存する</button>{' '}
                    <button type="button" className="w8-button" disabled={busy !== null} onClick={() => { void act(r.operation_id, 'discard'); }}>破棄する</button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </>
  );
}

export function FailedOpsPanel() {
  const state = useApiGet<CrmAdminOperationsResponse>('/api/admin/crm-operations?status=failed');
  const [gone, setGone] = useState<ReadonlySet<string>>(new Set());
  if (state.status === 'loading') return <p>読み込み中…</p>;
  if (state.status === 'error') {
    const t = describeApiError(state.error);
    return <p className="w8-alert" role="alert"><strong>{t.title}</strong> {t.detail}</p>;
  }
  const rows = state.data.operations.filter(o => !gone.has(o.operation_id));
  return <FailedOpsView rows={rows} onChanged={id => { setGone(prev => new Set(prev).add(id)); }} />;
}
