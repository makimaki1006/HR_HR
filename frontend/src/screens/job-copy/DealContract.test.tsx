// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { DealContract } from './DealContract';
import { DealSummary } from './DealSummary';
vi.mock('../../components/EChart', () => ({ EChart: () => null }));
import { contractCatalog, contractDetail, contractSchema, contractValues, contractProps } from './dealContract.testFixtures';
import { contractRequest, inputValue } from './dealContractModel';
import { buildPatchBody } from '../crm/writeModel';
import { clearCompanyDealsCache, loadCompanyDeals } from './companyDeals';

const response = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });
function mockApi(status = 200, error = '', pipeline = '21596025', enabled = true) {
  const patches: { body: ReturnType<typeof buildPatchBody>; headers: HeadersInit | undefined }[] = [];
  let values = { ...contractValues };
  const fetch = vi.fn((url: string, options?: RequestInit) => {
    if (url.includes('/api/job-copy/company-deals')) return Promise.resolve(response({ company: { id: '501', name: '合成事業所' }, deals: [{ id: '901', name: '合成案件', current: false, active: true, pipeline_id: pipeline, pipeline_label: '合成', stage_label: '合成・契約中', created_at: '2026-10-01', contract_start: values.contract_start_date.slice(0, 10), contract_end: values.contract_expiration_date }] }));
    if (url.includes('/api/job-copy/listings?')) return Promise.resolve(response({ listings: [{ id: '301', title: '合成求人', media: 'hrh', publication_status: '掲載中' }], next_offset: null }));
    if (url.includes('/api/job-copy/deal-applications')) return Promise.resolve(response({ listings: [{ listing_id: '301', by_date: {} }], shared_applications: 0, company: { id: '501', name: '合成事業所' } }));
    if (url.includes('listing-body-dates')) return Promise.resolve(response({ dates: {} }));
    if (url.includes('property-catalog')) return Promise.resolve(response(contractCatalog));
    if (url.includes('edit-schema')) return Promise.resolve(response({ ...contractSchema, pipeline_id: pipeline, writes_enabled: enabled }));
    if (url.includes('workspace/deals')) return Promise.resolve(response(contractDetail(values, pipeline)));
    if (url.includes('/operations/')) { values.amount = '360000'; return Promise.resolve(response({ status: 'saved' })); }
    if (options?.method === 'PATCH') {
      const body = JSON.parse(typeof options.body === 'string' ? options.body : '') as ReturnType<typeof buildPatchBody>;
      patches.push({ body, headers: options.headers });
      if (status === 200) { values = { ...values, ...body.set }; return Promise.resolve(response({ status: 'saved', operation_id: body.operation_id, values: body.set, objects_values: {} })); }
      if (status === 202) return Promise.resolve(response({ status: 'queued', operation_id: body.operation_id }, 202));
      if (status === 409) return Promise.resolve(response({ status: 'conflict', object: 'deal', current: { amount: '300000' }, changed_by_hubspot: ['amount'] }, 409));
      if (status === 422) return Promise.resolve(response({ status: 'invalid', errors: { amount: '金額は500000以下で入力してください。' }, missing_required: [] }, 422));
      return Promise.resolve(response(error === 'hubspot_busy' ? { error_kind: error } : { error }, status));
    }
    return Promise.resolve(response({ deals: [] }));
  });
  vi.stubGlobal('fetch', fetch);
  return { fetch, patches };
}
afterEach(() => { cleanup(); vi.unstubAllGlobals(); clearCompanyDealsCache(); });
async function editAmount() {
  await screen.findByRole('button', { name: '編集' });
  fireEvent.click(screen.getByRole('button', { name: '編集' }));
  fireEvent.change(screen.getByLabelText('金額', { exact: true }), { target: { value: '360000' } });
  fireEvent.click(screen.getByRole('button', { name: /^保存$/ }));
}

describe('契約・成果の差分', () => {
  it('変更項目だけを送り、base は読み取り時の値、日付・数値・選択肢は文字列、消去はnull', () => {
    const req = contractRequest('901', contractProps, contractValues, { amount: '360000', contract_plan: 'premium', contract_start_date: '2026-11-01', contract_period: '', oubo: '0', mensetu: '8' });
    expect(buildPatchBody(req, 'synthetic-operation-01')).toEqual({ operation_id: 'synthetic-operation-01', base: { amount: '240000', contract_plan: 'basic', contract_start_date: '2026-10-01T00:00:00Z', contract_period: '12', oubo: '20' }, set: { amount: '360000', contract_plan: 'premium', contract_start_date: '2026-11-01', contract_period: null, oubo: '0' } });
    expect(contractRequest('901', contractProps, contractValues, { contract_start_date: '2026-10-01', sales_amount: '1' }).changes).toEqual([]);
    const dateProp = contractProps.find(prop => prop.name === 'contract_start_date');
    if (!dateProp) throw new Error('合成の日付項目がありません');
    expect(inputValue(dateProp, '1790812800000')).toBe('2026-10-01');
  });
});

describe('実APIクライアントと既存の書き込みhookを使う表示', () => {
  for (const pipeline of ['21596025', '743961896']) it(`${pipeline}: 200は緑。選択は表示名からvalueへ、保存後に読み直す`, async () => {
    const { patches, fetch } = mockApi(200, '', pipeline);
    const saved = vi.fn();
    const { container } = render(<DealContract dealId="901" onSaved={saved} />);
    await editAmount();
    await screen.findByText('保存済み');
    expect(container.querySelector('.dc-green')?.textContent).toBe('保存済み');
    expect(screen.getByText('360000')).toBeTruthy();
    expect(patches[0]?.body.base).toEqual({ amount: '240000' });
    expect(patches[0]?.body.set).toEqual({ amount: '360000' });
    expect(patches[0]?.body.operation_id).toMatch(/^[a-zA-Z0-9_-]{8,64}$/);
    expect(new Headers(patches[0]?.headers).get('X-Requested-With')).toBe('fetch');
    expect(saved).toHaveBeenCalledOnce();
    await waitFor(() => { expect(fetch.mock.calls.filter(call => call[0].includes('workspace/deals')).length).toBe(2); });
    fireEvent.click(screen.getByRole('button', { name: '編集' }));
    expect(screen.getByLabelText<HTMLInputElement>('金額', { exact: true }).value).toBe('360000');
    fireEvent.change(screen.getByLabelText('契約プラン'), { target: { value: 'premium' } });
    fireEvent.click(screen.getByRole('button', { name: /^保存$/ }));
    await waitFor(() => { expect(patches).toHaveLength(2); });
    expect(patches[1]?.body.base).toEqual({ contract_plan: 'basic' });
    expect(patches[1]?.body.set).toEqual({ contract_plan: 'premium' });
    expect(screen.queryByLabelText('売上額', { selector: 'input' })).toBeNull();
    expect(screen.queryByLabelText('分割売上額', { selector: 'input' })).toBeNull();
  });
  it('202は黄で入力を止め、確認APIで保存を確かめてから緑', async () => {
    const { fetch } = mockApi(202);
    const saved = vi.fn();
    const queued = vi.fn();
    const { container } = render(<DealContract dealId="901" onSaved={saved} onQueued={queued} />);
    await editAmount();
    await screen.findByText('一時保存済み・HubSpotへの反映待ち');
    expect(container.querySelector('.dc-yellow')?.textContent).toContain('反映待ち');
    expect(screen.getByRole('button', { name: '編集' }).hasAttribute('disabled')).toBe(true);
    expect(saved).not.toHaveBeenCalled();
    expect(queued).toHaveBeenCalledOnce();
    await screen.findByText('保存済み', {}, { timeout: 7000 });
    expect(fetch.mock.calls.some(call => call[0].includes('/operations/'))).toBe(true);
    expect(saved).toHaveBeenCalledOnce();
  }, 10000);
  it('409は現在値300000と入力360000を示し、自動上書きせず現在値を選べる', async () => {
    const { patches } = mockApi(409);
    render(<DealContract dealId="901" onSaved={vi.fn()} />);
    await editAmount();
    await screen.findByText(/現在の値「300000」／入力した値「360000」/);
    expect(patches).toHaveLength(1);
    fireEvent.click(screen.getByRole('button', { name: '現在の値を使う' }));
    expect(screen.getByText('300000')).toBeTruthy();
    expect(patches).toHaveLength(1);
  });
  it('409で入力値を選んだ場合だけ、現在値をbaseに新しい操作で再送する', async () => {
    const { patches } = mockApi(409);
    render(<DealContract dealId="901" onSaved={vi.fn()} />);
    await editAmount();
    fireEvent.click(await screen.findByRole('button', { name: '確認して入力した値で保存' }));
    await waitFor(() => { expect(patches).toHaveLength(2); });
    expect(patches[1]?.body.base).toEqual({ amount: '300000' });
    expect(patches[1]?.body.set).toEqual({ amount: '360000' });
    expect(patches[1]?.body.operation_id).not.toBe(patches[0]?.body.operation_id);
  });
  it('422は具体的な入力エラーを赤で表示し、入力を残す', async () => {
    mockApi(422);
    const { container } = render(<DealContract dealId="901" onSaved={vi.fn()} />);
    await editAmount();
    await screen.findByText('金額は500000以下で入力してください。');
    expect(container.querySelector('.dc-red')?.textContent).toBe('金額は500000以下で入力してください。');
    expect(screen.getByLabelText<HTMLInputElement>('金額', { exact: true }).value).toBe('360000');
  });
  for (const error of ['hubspot_busy', 'record_busy', 'queue_full', 'queue_unavailable']) it(`503 ${error}は何も保存されていないと赤で表示`, async () => {
    mockApi(503, error);
    const saved = vi.fn();
    const { container } = render(<DealContract dealId="901" onSaved={saved} />);
    await editAmount();
    await screen.findByText(/何も保存されていません/);
    expect(container.querySelector('.dc-red')?.textContent).toContain('何も保存されていません');
    expect(saved).not.toHaveBeenCalled();
  });
  it('通信結果が不明な場合は未保存と断定せず、同じ操作で再試行する', async () => {
    const { patches } = mockApi(503, 'queue_uncertain');
    render(<DealContract dealId="901" onSaved={vi.fn()} />);
    await editAmount();
    await screen.findByText(/保存できたか確認できません/);
    fireEvent.click(screen.getByRole('button', { name: /^保存$/ }));
    await waitFor(() => { expect(patches).toHaveLength(2); });
    expect(patches[1]?.body.operation_id).toBe(patches[0]?.body.operation_id);
    expect(screen.queryByText(/何も保存されていません/)).toBeNull();
  });
  it('本番の栓が閉じていれば値を表示し、編集を出さない', async () => {
    mockApi(200, '', '21596025', false);
    render(<DealContract dealId="901" onSaved={vi.fn()} />);
    await screen.findByText('240000');
    expect(screen.getByText('合成・契約中', { exact: false })).toBeTruthy();
    expect(screen.getByText('この案件はまだ編集できません（試験運用中）')).toBeTruthy();
    expect(screen.queryByRole('button', { name: '編集' })).toBeNull();
  });
  it('対象外パイプラインは表示のみ', async () => {
    mockApi(200, '', 'other');
    render(<DealContract dealId="901" onSaved={vi.fn()} />);
    await screen.findByText('この取引は契約・成果の編集対象外です。');
    expect(screen.queryByRole('button', { name: '編集' })).toBeNull();
  });
  it('案件画面からの保存で取引キャッシュを破棄し、契約日を取り直す', async () => {
    const { fetch } = mockApi();
    await loadCompanyDeals('501');
    const saved = vi.fn();
    render(<DealSummary deal={{ id: '901', name: '合成案件', prefecture: '', title: '', media: '' }} billingByJob={{}} onOpenListing={vi.fn()} onContractSaved={saved} />);
    await screen.findByText(/取引の状態/);
    expect(fetch.mock.calls.filter(call => call[0].includes('company-deals'))).toHaveLength(1);
    fireEvent.click(await screen.findByRole('button', { name: '編集' }));
    fireEvent.change(screen.getByLabelText('契約開始日'), { target: { value: '2026-11-01' } });
    fireEvent.click(screen.getByRole('button', { name: /^保存$/ }));
    await screen.findByText('保存済み');
    await waitFor(() => { expect(fetch.mock.calls.filter(call => call[0].includes('company-deals'))).toHaveLength(2); });
    expect(saved).toHaveBeenCalledOnce();
    const deals = await loadCompanyDeals('501');
    expect(deals.ok && deals.data.deals[0]?.contract_start).toBe('2026-11-01');
    expect(fetch.mock.calls.filter(call => call[0].includes('company-deals'))).toHaveLength(2);
  });
});
