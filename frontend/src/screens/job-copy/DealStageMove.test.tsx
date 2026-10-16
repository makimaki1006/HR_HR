// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { DealContract } from './DealContract';
import { contractCatalog, contractDetail, contractValues, stageSchema } from './dealContract.testFixtures';
import type { PatchRequest } from '../crm/writeTypes';

const response = (data: unknown, status = 200) => new Response(JSON.stringify(data), { status, headers: { 'content-type': 'application/json' } });
function setup(failure = '', pipeline = '21596025', populated = false) {
  const patches: PatchRequest[] = [];
  let stage = 'stage-demo';
  let values = { ...contractValues, oubo: populated ? '20' : '', mensetu: populated ? '8' : '' };
  let failRead = false;
  const fetch = vi.fn((url: string, options?: RequestInit) => {
    return Promise.resolve((() => {
    if (url.includes('/refresh')) return response({ status: 'refreshed' });
    if (url.includes('/operations/')) return response({ status: 'saved' });
    if (url.includes('property-catalog')) return response(contractCatalog);
    if (url.includes('edit-schema')) return response({ ...stageSchema, pipeline_id: pipeline, stages: pipeline === '743961896' ? stageSchema.stages.map(s => ({ ...s, required: [], shown: [] })) : stageSchema.stages });
    if (url.includes('workspace/deals')) {
      if (failRead) return response({}, 503);
      return response({ ...contractDetail(values, pipeline), deal: { ...contractDetail(values, pipeline).deal, stage_id: stage, stage_label: stageSchema.stages.find(s => s.id === stage)?.label } });
    }
    if (options?.method === 'PATCH') {
      const body = JSON.parse(typeof options.body === 'string' ? options.body : '') as PatchRequest; patches.push(body);
      if (failure === 'queued') return response({ status: 'queued', operation_id: body.operation_id }, 202);
      if (failure === 'missing_required') return response({ status: 'invalid', missing_required: ['oubo'], errors: { oubo: '移動先のステージで必須です' } }, 422);
      if (failure === 'invalid_stage') return response({ status: 'invalid', missing_required: [], errors: { stage: '納品管理のステージ移動は同じパイプライン内だけ許可されています' } }, 422);
      if (failure === 'conflict') return response({ status: 'conflict', object: 'deal', current: { oubo: '40' }, changed_by_hubspot: ['oubo'] }, 409);
      values = { ...values, ...body.set }; stage = body.stage?.stage_id ?? stage;
      return response({ status: 'saved', values: body.set, objects_values: {} });
    }
    throw new Error(`未定義の合成API: ${url}`);
    })());
  });
  vi.stubGlobal('fetch', fetch);
  const saved = vi.fn();
  const view = render(<DealContract dealId="901" onSaved={saved} />);
  return { patches, saved, ...view, failRead: () => { failRead = true; } };
}
afterEach(() => { cleanup(); vi.unstubAllGlobals(); });
async function select(id: string) {
  fireEvent.change(await screen.findByLabelText('ステージを変更'), { target: { value: id } });
  return screen.findByRole('dialog');
}
async function fillRequired(dialog: HTMLElement) {
  await within(dialog).findByLabelText('応募数 (必須)');
  fireEvent.change(within(dialog).getByLabelText('応募数 (必須)'), { target: { value: '25' } });
  fireEvent.change(within(dialog).getByLabelText('面接数 (必須)'), { target: { value: '10' } });
}
it('schemaの順で同じ区分だけ選び、必須入力と未保存の契約を1回のPATCHにまとめる', async () => {
  const { patches, saved, container } = setup();
  fireEvent.click(await screen.findByRole('button', { name: '編集' }));
  fireEvent.change(screen.getByLabelText('金額', { exact: true }), { target: { value: '360000' } });
  expect([...screen.getByLabelText<HTMLSelectElement>('ステージを変更').options].map(o => o.value)).toEqual(['stage-demo', '66848546', 'delivery-done', 'cancelled']);
  const dialog = await select('66848546');
  await within(dialog).findByText('必須 あと 2 項目', { exact: false });
  expect(within(dialog).getByRole('button', { name: '移す' }).getAttribute('aria-disabled')).toBe('true');
  expect(patches).toHaveLength(0);
  await fillRequired(dialog);
  fireEvent.click(within(dialog).getByRole('button', { name: '移す' }));
  await screen.findByText('保存済み');
  expect(patches).toHaveLength(1);
  expect(patches[0]?.operation_id).toMatch(/^[a-zA-Z0-9_-]{8,64}$/);
  expect(patches[0]).toMatchObject({ stage: { pipeline_id: '21596025', stage_id: '66848546' }, base: { oubo: '', mensetu: '', amount: '240000' }, set: { oubo: '25', mensetu: '10', amount: '360000' } });
  expect(container.querySelector('.dc-green')?.textContent).toBe('保存済み');
  expect(screen.getByLabelText<HTMLSelectElement>('ステージを変更').value).toBe('66848546');
  expect(saved).toHaveBeenCalledOnce();
});
for (const id of ['delivery-done', 'cancelled']) it(`${id}: 必須なしでも確定前に1回確認し、取消では送らない`, async () => {
  const { patches } = setup();
  let dialog = await select(id);
  expect(within(dialog).getByText(/よろしいですか/)).toBeTruthy();
  fireEvent.click(within(dialog).getByRole('button', { name: 'やめる' }));
  expect(patches).toHaveLength(0);
  dialog = await select(id);
  fireEvent.click(within(dialog).getByRole('button', { name: '移す' }));
  await screen.findByText('保存済み');
  expect(patches[0]?.operation_id).toMatch(/^[a-zA-Z0-9_-]{8,64}$/);
  expect(patches[0]).toMatchObject({ stage: { pipeline_id: '21596025', stage_id: id }, base: {}, set: {} });
});
it('請負契約の必須なしステージも選べる', async () => {
  const { patches } = setup('', '743961896');
  const dialog = await select('66848546');
  fireEvent.click(within(dialog).getByRole('button', { name: '移す' }));
  await screen.findByText('保存済み');
  expect(patches[0]?.stage).toEqual({ pipeline_id: '743961896', stage_id: '66848546' });
});
for (const failure of ['missing_required', 'invalid_stage']) it(`422 ${failure}は日本語で赤表示し、入力を残す`, async () => {
  const { container } = setup(failure);
  const dialog = await select('66848546'); await fillRequired(dialog);
  fireEvent.click(within(dialog).getByRole('button', { name: '移す' }));
  await waitFor(() => { expect(container.querySelector('.dc-red')?.textContent).toContain(failure === 'missing_required' ? '移動に必要な項目が未入力' : '同じ取引区分のステージ'); });
  if (failure === 'missing_required') await within(dialog).findByText('次の項目を入力してください: 応募数');
  expect(within(dialog).getByLabelText<HTMLInputElement>('応募数 (必須)').value).toBe('25');
});
it('必須項目の読取失敗時はダイアログを開かず書かない', async () => {
  const { failRead, patches } = setup();
  await screen.findByLabelText('ステージを変更'); failRead();
  fireEvent.change(screen.getByLabelText('ステージを変更'), { target: { value: '66848546' } });
  await screen.findByText(/移動に必要な現在の値を取得できませんでした/);
  expect(screen.queryByRole('dialog')).toBeNull(); expect(patches).toHaveLength(0);
});
it('競合時は現在の応募数40件を示し、確認後のbaseも40で再送', async () => {
  const { patches } = setup('conflict');
  const dialog = await select('66848546'); await fillRequired(dialog);
  fireEvent.click(within(dialog).getByRole('button', { name: '移す' }));
  await screen.findByText('応募数：現在の値「40件」／入力した値「25件」');
  expect(patches).toHaveLength(1);
  fireEvent.click(screen.getByRole('button', { name: '確認して入力した値で保存' }));
  await waitFor(() => { expect(patches).toHaveLength(2); });
  expect(patches[1]?.base.oubo).toBe('40');
});

it('必須項目が入力済みなら元値を表示し、未変更項目は書き換えない', async () => {
  const { patches } = setup('', '21596025', true);
  const dialog = await select('66848546');
  await within(dialog).findByText('必須項目はすべて入力済みです');
  expect(within(dialog).getByRole<HTMLInputElement>('spinbutton', { name: '応募数' }).value).toBe('20');
  fireEvent.click(within(dialog).getByRole('button', { name: '移す' }));
  await screen.findByText('保存済み');
  expect(patches[0]?.base).toEqual({}); expect(patches[0]?.set).toEqual({});
});
it('反映待ち中はステージを黄で表示して再操作を止め、反映後にキャッシュを読み直す', async () => {
  const { container, saved } = setup('queued');
  const dialog = await select('delivery-done');
  fireEvent.click(within(dialog).getByRole('button', { name: '移す' }));
  await screen.findByText('一時保存済み・HubSpotへの反映待ち');
  expect(container.querySelector('.dc-yellow')?.textContent).toContain('反映待ち');
  const selector = screen.getByLabelText<HTMLSelectElement>('ステージを変更');
  expect(selector.value).toBe('delivery-done'); expect(selector.disabled).toBe(true);
  expect(saved).not.toHaveBeenCalled();
  await screen.findByText('保存済み', {}, { timeout: 7000 });
  await waitFor(() => { expect(saved).toHaveBeenCalledOnce(); });
}, 10000);
it('契約欄と必須入力が重なる応募数は入力を引き継ぎ、ダイアログの最終値と編集前のbaseを送る', async () => {
  const { patches } = setup();
  fireEvent.click(await screen.findByRole('button', { name: '編集' }));
  fireEvent.change(screen.getByLabelText('応募数', { exact: true }), { target: { value: '25' } });
  const dialog = await select('66848546');
  await waitFor(() => { expect(within(dialog).getByRole<HTMLInputElement>('spinbutton', { name: '応募数' }).value).toBe('25'); });
  fireEvent.change(within(dialog).getByRole('spinbutton', { name: '応募数' }), { target: { value: '30' } });
  fireEvent.change(within(dialog).getByRole('spinbutton', { name: '面接数' }), { target: { value: '12' } });
  fireEvent.click(within(dialog).getByRole('button', { name: '移す' }));
  await screen.findByText('保存済み');
  expect(patches).toHaveLength(1);
  expect(patches[0]?.base).toEqual({ oubo: '', mensetu: '' });
  expect(patches[0]?.set).toEqual({ oubo: '30', mensetu: '12' });
});
