// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { CrmAdminOperation } from '../../generated/CrmAdminOperation';
import { failureReason, FailedOpsView } from './FailedOpsPanel';

const post = vi.hoisted(() => vi.fn());
vi.mock('../../api/client', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../api/client')>()),
  apiPost: post,
}));
afterEach(() => { cleanup(); vi.resetAllMocks(); });

const row: CrmAdminOperation = {
  operation_id: 'op-1', deal_id: '9001', operator_email: 'bpo@f-a-c.co.jp', props: ['bpo_50', 'dealstage'],
  status: 'failed', error: 'conflict', attempts: 2, created_at: '2026-10-25T01:00:00Z', updated_at: '2026-10-25T01:02:03Z', next_retry_at: null,
};

describe('FailedOpsView', () => {
  it('理由を平易な日本語にし、内部のコードは出さない', () => {
    expect(failureReason('conflict')).toBe('保存しようとしたあとで、HubSpot の値が変わっていました');
    render(<FailedOpsView rows={[row]} onChanged={() => undefined} />);
    expect(screen.getByTestId('failed-ops-table').textContent).toContain('bpo_50、dealstage');
    expect(document.body.textContent).not.toContain('conflict');
  });

  it('0 件なら「ありません」', () => {
    render(<FailedOpsView rows={[]} onChanged={() => undefined} />);
    expect(screen.getByTestId('failed-ops-empty').textContent).toBe('保存できなかった操作はありません。');
  });

  it('破棄すると discard を POST して行を外す通知を出す', async () => {
    post.mockResolvedValue({ ok: true, data: {} });
    const onChanged = vi.fn();
    render(<FailedOpsView rows={[row]} onChanged={onChanged} />);
    fireEvent.click(screen.getByText('破棄する'));
    await waitFor(() => { expect(onChanged).toHaveBeenCalledWith('op-1'); });
    expect(post).toHaveBeenCalledWith('/api/admin/crm-operations/op-1/discard', {});
  });
});
