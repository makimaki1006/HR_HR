// @vitest-environment happy-dom
import { cleanup, render, screen, fireEvent, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { HubSpotReadPanel } from './HubSpotReadPanel';
import { HrhPerformance } from './HrhPerformance';
import { ConsultantReview } from './ConsultantReview';
import { AbComparison } from './AbComparison';
import { BillingImportPanel } from './BillingImportPanel';
import type { JobCopyRecord } from './data';
vi.mock('../../components/EChart', () => ({ EChart: () => <div>グラフ</div> }));
const api = vi.hoisted(() => vi.fn());
vi.mock('../../api/client', async original => ({ ...await original<typeof import('../../api/client')>(), apiGet: api }));
afterEach(() => { cleanup(); vi.resetAllMocks(); });
const job: JobCopyRecord = { id: 'hubspot-history-901234567890', hubspotId: '901234567890', accountId: 'SHOP-SECRET-001', mediaJobId: 'HR-1', dataSource: 'hubspot', title: '合成の配送求人', company: '合成の取引先', media: 'HRハッカー', location: '大分県大分市', versions: [{ id: 'v1', label: '取得した版1', observedAt: '2026-10-01T00:00:00Z', body: '仕事内容：配送します', kind: 'published', certainty: 'unknown', source: '保存された文面', applications: null, note: '' }] };
const forbidden = /HR-|901234567890|SHOP-SECRET|求人ID|店舗ID|HubSpot ID/;
describe('媒体名と求人名で示す', () => {
  it.each([false, true])('課金の取得有無 %s でも照合番号を表示しない', available => {
    const record = available ? { ...job, hrhPerformance: { schema_version: 1 as const, source: 'hrhacker' as const, job_id: 'HR-1', captured_at: '2026-10-10T00:00:00Z', rows: [{ period_start: '2026-10-01', period_end: '2026-10-09', impressions: 100, clicks: 10, cost_yen: 1000, applications: 2 }] } } : job;
    const { container } = render(<HrhPerformance job={record} />);
    expect(container.textContent).not.toMatch(forbidden);
    expect(container.textContent).toContain('合成の配送求人');
    if (available) expect(container.textContent).toContain('1,000円');
  });
  it('顧客への報告に内部番号を含めない', () => {
    const { container } = render(<ConsultantReview job={job} draft={{ stage: 'before', target: '', fields: {}, selection: ['v1', 'v1'] }} onDraft={() => undefined} />);
    expect(container.textContent).not.toMatch(forbidden);
    expect(container.textContent).toContain('HRハッカー');
    expect(container.textContent).toContain('合成の配送求人');
  });
  it('A/Bの本文と選択候補に内部番号を含めない', () => {
    const { container } = render(<AbComparison job={job} records={[job, { ...job, id: 'other', title: '合成の夜間配送求人', mediaJobId: 'HR-2' }]} />);
    expect(container.textContent).not.toMatch(forbidden);
    expect(screen.getByRole('option', { name: /合成の夜間配送求人/ }).textContent).toContain('HRハッカー');
  });
  it('課金CSVの照合用の値やエラーにも内部番号を露出しない', async () => {
    const { container } = render(<BillingImportPanel records={[job]} applied={[]} onApply={() => undefined} onClear={() => undefined} />);
    fireEvent.change(screen.getByLabelText('課金CSVファイル'), { target: { files: [new File(['媒体,店舗ID,媒体求人ID,期間開始,期間終了,金額\nHRハッカー,SHOP-SECRET-001,HR-1,2026-10-01,2026-10-09,1000'], 'sample.csv', { type: 'text/csv' })] } });
    await waitFor(() => { expect(screen.getByRole('button', { name: '求人と照合する' })).toBeTruthy(); });
    fireEvent.click(screen.getByRole('button', { name: '求人と照合する' }));
    await waitFor(() => { expect(container.textContent).toContain('8桁の数字'); });
    expect(container.textContent).not.toMatch(forbidden);
  });
  it('取引先確認の契約名が欠けても生のレコード番号で代用しない', async () => {
    api.mockImplementation((path: string) => Promise.resolve({ ok: true, data: path.includes('company=') ? { company_id: '77', contracts: [{ id: '901234567890', properties: { code_of_customer: 'SHOP-SECRET-001' } }], jobs: [], total: 0, next_offset: null } : { customers: [{ id: '77', properties: { name: '合成の取引先' } }], next_after: null } }));
    const { container } = render(<HubSpotReadPanel onOpen={() => undefined} />);
    fireEvent.click(screen.getByRole('button', { name: '取引先を取得' }));
    await screen.findByRole('option', { name: '合成の取引先' });
    fireEvent.change(screen.getByLabelText('HubSpotの取引先'), { target: { value: '77' } });
    fireEvent.click(screen.getByRole('button', { name: '関連する求人を取得' }));
    await waitFor(() => { expect(screen.getByLabelText('契約で絞り込む')).toBeTruthy(); });
    expect(container.textContent).not.toMatch(forbidden);
    expect(screen.getByRole('option', { name: '契約名未取得' })).toBeTruthy();
  });

});
