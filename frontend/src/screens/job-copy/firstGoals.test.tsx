// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { DealFirstGoals } from './DealFirstGoals';
import { GOAL_PROPS, confirmedData, goalLine, goalProblems, goalsRequest, readGoals } from './firstGoalsModel';
import type { GoalData } from './firstGoalsModel';
import { buildPatchBody } from '../crm/writeModel';
import { contractCatalog, contractDetail, contractSchema, contractValues } from './dealContract.testFixtures';

const draft: GoalData = {
  v: 1, status: 'draft',
  goals: [
    { kind: 'ideal', what: '合成ドライバーの応募を集める', target: '4名', by_when: '契約期間中' },
    { kind: 'minimum', what: '合成ドライバーの応募を集める', target: '2名', by_when: '契約期間中' },
    { kind: 'unclassified', what: '合成ドライバーを採用する', target: '2名', by_when: '' },
  ],
  meeting: { start_jst: '2026-10-19 10:55', topic: '【初回MTG】合成株式会社', sources: ['slide'], notes: ['文字起こしを取得できませんでした'] },
};
const stored = (data: GoalData) => ({
  [GOAL_PROPS.ideal]: data.goals.filter(goal => goal.kind === 'ideal').map(goalLine).join('\n') || null,
  [GOAL_PROPS.minimum]: data.goals.filter(goal => goal.kind === 'minimum').map(goalLine).join('\n') || null,
  [GOAL_PROPS.data]: JSON.stringify(data),
});

describe('初回MTGの目標の読み書き', () => {
  it('JSON と文が揃っていれば JSON を読み、区分不明も残す', () => {
    const read = readGoals(stored(draft));
    expect(read?.status).toBe('draft');
    expect(read?.goals.map(goal => goal.kind)).toEqual(['ideal', 'minimum', 'unclassified']);
    expect(read?.meeting?.notes).toEqual(['文字起こしを取得できませんでした']);
  });
  it('HubSpot で文を直していれば文を正として確定扱いにする', () => {
    const values = { ...stored(draft), [GOAL_PROPS.ideal]: '合成ドライバーを3名採用する（3名）／2027/01/11まで\n応募を月4件' };
    const read = readGoals(values);
    expect(read?.status).toBe('confirmed');
    expect(read?.goals.filter(goal => goal.kind === 'ideal')).toEqual([
      { kind: 'ideal', what: '合成ドライバーを3名採用する', target: '3名', by_when: '2027/01/11まで' },
      { kind: 'ideal', what: '応募を月4件', target: '', by_when: '' },
    ]);
  });
  it('何も無ければ null、壊れた JSON でも文だけで読める', () => {
    expect(readGoals({})).toBeNull();
    expect(readGoals({ [GOAL_PROPS.data]: '{壊れ', [GOAL_PROPS.minimum]: '応募を1件以上集める（1件）／契約期間中' })?.goals).toEqual([
      { kind: 'minimum', what: '応募を1件以上集める', target: '1件', by_when: '契約期間中' },
    ]);
  });
  it('何をするかが空・区分不明があると保存できない。いつまでが空なら契約期間中で保存する', () => {
    expect(goalProblems(draft.goals)).toEqual(['区分不明の目標があります。理想か前進ラインを選んでください。']);
    expect(goalProblems([{ kind: 'ideal', what: ' ', target: '', by_when: '' }])).toEqual(['「何をするか」が空の目標があります。書くか、行を消してください。']);
    const data = confirmedData([{ kind: 'minimum', what: ' 応募を集める ', target: '2名', by_when: '' }], draft, new Date('2026-10-23T00:00:00Z'));
    expect(data).toEqual({ v: 1, status: 'confirmed', goals: [{ kind: 'minimum', what: '応募を集める', target: '2名', by_when: '契約期間中' }], meeting: draft.meeting, updated_at: '2026-10-23T00:00:00.000Z' });
  });
  it('3 項目を読み取り時の値を base にしてまとめて送る。変わらない項目は送らない', () => {
    const base = stored(draft);
    const next = confirmedData(draft.goals.map(goal => (goal.kind === 'unclassified' ? { ...goal, kind: 'ideal' as const } : goal)), draft, new Date('2026-10-23T00:00:00Z'));
    const body = buildPatchBody(goalsRequest('901', base, next), 'synthetic-op');
    expect(Object.keys(body.set).sort()).toEqual([GOAL_PROPS.data, GOAL_PROPS.ideal].sort());
    expect(body.base[GOAL_PROPS.ideal]).toBe(base[GOAL_PROPS.ideal]);
    expect(body.set[GOAL_PROPS.ideal]).toBe('合成ドライバーの応募を集める（4名）／契約期間中\n合成ドライバーを採用する（2名）／契約期間中');
    expect(JSON.parse(body.set[GOAL_PROPS.data] ?? '')).toMatchObject({ status: 'confirmed' });
  });
});

const response = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });
function mockApi(values: Record<string, string | null>, withProps = true) {
  const patches: ReturnType<typeof buildPatchBody>[] = [];
  const goalProps = Object.values(GOAL_PROPS).map(name => ({ ...contractCatalog.objects[0]?.groups[0]?.properties[0], name, label: name, property_type: 'string', field_type: 'textarea', options: [] }));
  const catalog = withProps ? { ...contractCatalog, objects: contractCatalog.objects.map(object => ({ ...object, groups: [...object.groups, { name: '納品管理', label: '納品管理', properties: goalProps }] })) } : contractCatalog;
  const schema = { ...contractSchema, editable: [...contractSchema.editable, ...Object.values(GOAL_PROPS).map(name => ({ ...contractSchema.editable[0], object: 'deal', name }))] };
  vi.stubGlobal('fetch', vi.fn((url: string, options?: RequestInit) => {
    if (url.includes('property-catalog')) return Promise.resolve(response(catalog));
    if (url.includes('edit-schema')) return Promise.resolve(response({ ...schema, pipeline_id: '21596025', writes_enabled: true }));
    if (url.includes('workspace/deals')) { const detail = contractDetail(contractValues, '21596025'); return Promise.resolve(response({ ...detail, selected: { ...detail.selected, deal: values } })); }
    if (options?.method === 'PATCH') {
      const body = JSON.parse(typeof options.body === 'string' ? options.body : '') as ReturnType<typeof buildPatchBody>;
      patches.push(body);
      Object.assign(values, body.set);
      return Promise.resolve(response({ status: 'saved', operation_id: body.operation_id, values: body.set, objects_values: {} }));
    }
    return Promise.resolve(response({}));
  }));
  return patches;
}
afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

describe('初回MTGの目標の画面', () => {
  it('MTGから読んだ案を出し、区分不明を選び直すまで保存させない。確定して保存すると 1 回で送る', async () => {
    const patches = mockApi({ ...stored(draft) });
    render(<DealFirstGoals dealId="901" />);
    expect(await screen.findByText(/MTGから読み取った案です/)).toBeTruthy();
    expect(screen.getByText('文字起こしを取得できませんでした')).toBeTruthy();
    expect(screen.getByText(/画面に映した目標の資料/)).toBeTruthy();
    fireEvent.click(await screen.findByRole('button', { name: '確かめて確定' }));
    const save = screen.getByRole('button', { name: '確定して保存' });
    expect((save as HTMLButtonElement).disabled).toBe(true);
    const [, , third] = screen.getAllByLabelText('区分');
    if (!third) throw new Error('区分不明の行がありません');
    fireEvent.change(third, { target: { value: 'ideal' } });
    expect((save as HTMLButtonElement).disabled).toBe(false);
    fireEvent.click(save);
    await waitFor(() => { expect(patches).toHaveLength(1); });
    expect(JSON.parse(patches[0]?.set[GOAL_PROPS.data] ?? '')).toMatchObject({ status: 'confirmed' });
    expect(await screen.findByRole('button', { name: '訂正' })).toBeTruthy();
  });
  it('HubSpot に項目が無ければその旨だけ出す', async () => {
    mockApi({}, false);
    render(<DealFirstGoals dealId="901" />);
    expect(await screen.findByText('HubSpot に初回MTGの目標の項目がまだありません。')).toBeTruthy();
  });
  it('何も入っていなければ入力から始め、理想と前進ラインの 2 行を出す', async () => {
    mockApi({});
    render(<DealFirstGoals dealId="901" />);
    fireEvent.click(await screen.findByRole('button', { name: '入力' }));
    expect(screen.getAllByLabelText('何をするか')).toHaveLength(2);
    expect(screen.getByRole('alert').textContent).toContain('「何をするか」が空の目標があります');
  });
});
