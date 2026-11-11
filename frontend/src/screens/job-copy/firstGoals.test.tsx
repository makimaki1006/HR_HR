// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { DealFirstGoals } from './DealFirstGoals';
import { GOAL_PROPS, confirmDraft, confirmedData, goalLine, goalProblems, goalsRequest, readGoals, textEditedInHubSpot } from './firstGoalsModel';
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
    expect(goalProblems(draft.goals)).toEqual(['区分不明の目標があります。マインド目標かコミット目標を選んでください。']);
    expect(goalProblems([{ kind: 'ideal', what: ' ', target: '', by_when: '' }])).toEqual(['「何をするか」が空の目標があります。書くか、行を消してください。']);
    const data = confirmedData([{ kind: 'minimum', what: ' 応募を集める ', target: '2名', by_when: '' }], draft, new Date('2026-10-23T00:00:00Z'));
    expect(data).toEqual({ v: 1, status: 'confirmed', goals: [{ kind: 'minimum', what: '応募を集める', target: '2名', by_when: '契約期間中' }], meeting: draft.meeting, updated_at: '2026-10-23T00:00:00.000Z' });
  });
  it('3 項目を読み取り時の値を base にして必ずまとめて送る (どれか 1 つでも HubSpot で変わっていれば上書きしない)', () => {
    const base = stored(draft);
    const next = confirmedData(draft.goals.map(goal => (goal.kind === 'unclassified' ? { ...goal, kind: 'ideal' as const } : goal)), draft, new Date('2026-10-23T00:00:00Z'));
    const body = buildPatchBody(goalsRequest('901', base, next), 'synthetic-op');
    expect(Object.keys(body.set).sort()).toEqual([GOAL_PROPS.data, GOAL_PROPS.ideal, GOAL_PROPS.minimum].sort());
    expect(body.base).toEqual(base);
    expect(body.set[GOAL_PROPS.minimum]).toBe(base[GOAL_PROPS.minimum]);
    expect(body.set[GOAL_PROPS.ideal]).toBe('合成ドライバーの応募を集める（4名）／契約期間中\n合成ドライバーを採用する（2名）／契約期間中');
    expect(JSON.parse(body.set[GOAL_PROPS.data] ?? '')).toMatchObject({ status: 'confirmed' });
  });
  it('確定済みで中身が同じなら何も送らない。案の確定は文を変えずに確定済みにする', () => {
    const confirmed = confirmedData(draft.goals.filter(goal => goal.kind !== 'unclassified'), draft, new Date('2026-10-23T00:00:00Z'));
    const again = confirmedData(confirmed.goals, confirmed, new Date('2026-10-24T00:00:00Z'));
    expect(goalsRequest('901', stored(confirmed), again).changes).toEqual([]);
    const plain: GoalData = { ...draft, goals: confirmed.goals };
    const body = buildPatchBody(goalsRequest('901', stored(plain), confirmDraft(plain, new Date('2026-10-23T00:00:00Z'))), 'synthetic-op');
    expect(body.set[GOAL_PROPS.ideal]).toBe(stored(plain)[GOAL_PROPS.ideal]);
    expect(body.set[GOAL_PROPS.minimum]).toBe(stored(plain)[GOAL_PROPS.minimum]);
    expect(JSON.parse(body.set[GOAL_PROPS.data] ?? '')).toMatchObject({ status: 'confirmed', meeting: draft.meeting });
  });
  it('目標を全部消して保存しても、確定済みの印としてデータを残す（夜のバッチが案を書き戻さない）', () => {
    const body = buildPatchBody(goalsRequest('901', stored(draft), confirmedData([], draft, new Date('2026-10-23T00:00:00Z'))), 'synthetic-op');
    expect(body.set[GOAL_PROPS.ideal]).toBeNull();
    expect(body.set[GOAL_PROPS.minimum]).toBeNull();
    expect(JSON.parse(body.set[GOAL_PROPS.data] ?? '')).toMatchObject({ status: 'confirmed', goals: [] });
    expect(readGoals(body.set)).toBeNull();
  });
  it('HubSpot で文だけ直された・消された場合は文を正とする', () => {
    const values = { ...stored(draft), [GOAL_PROPS.minimum]: '面接を月2件入れる' };
    expect(textEditedInHubSpot(values)).toBe(true);
    expect(textEditedInHubSpot(stored(draft))).toBe(false);
    const confirmed: GoalData = { ...draft, status: 'confirmed', goals: draft.goals.filter(goal => goal.kind !== 'unclassified') };
    expect(readGoals({ ...stored(confirmed), [GOAL_PROPS.ideal]: null, [GOAL_PROPS.minimum]: null })).toBeNull();
  });
});

const response = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });
function mockApi(values: Record<string, string | null>, withProps = true, conflictWith?: Record<string, string | null>) {
  const patches: ReturnType<typeof buildPatchBody>[] = [];
  const reads: string[] = [];
  const goalProps = Object.values(GOAL_PROPS).map(name => ({ ...contractCatalog.objects[0]?.groups[0]?.properties[0], name, label: name, property_type: 'string', field_type: 'textarea', options: [] }));
  const catalog = withProps ? { ...contractCatalog, objects: contractCatalog.objects.map(object => ({ ...object, groups: [...object.groups, { name: '納品管理', label: '納品管理', properties: goalProps }] })) } : contractCatalog;
  const schema = { ...contractSchema, editable: [...contractSchema.editable, ...Object.values(GOAL_PROPS).map(name => ({ ...contractSchema.editable[0], object: 'deal', name }))] };
  vi.stubGlobal('fetch', vi.fn((url: string, options?: RequestInit) => {
    if (url.includes('property-catalog')) return Promise.resolve(response(catalog));
    if (url.includes('edit-schema')) return Promise.resolve(response({ ...schema, pipeline_id: '21596025', writes_enabled: true }));
    if (url.includes('workspace/deals')) { reads.push(url); const detail = contractDetail(contractValues, '21596025'); return Promise.resolve(response({ ...detail, selected: { ...detail.selected, deal: values } })); }
    if (options?.method === 'PATCH') {
      const body = JSON.parse(typeof options.body === 'string' ? options.body : '') as ReturnType<typeof buildPatchBody>;
      patches.push(body);
      if (conflictWith) return Promise.resolve(response({ status: 'conflict', object: 'deal', current: { ...body.set, ...conflictWith }, changed_by_hubspot: Object.keys(conflictWith) }, 409));
      Object.assign(values, body.set);
      return Promise.resolve(response({ status: 'saved', operation_id: body.operation_id, values: body.set, objects_values: {} }));
    }
    return Promise.resolve(response({}));
  }));
  return Object.assign(patches, { reads });
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
    const save = await screen.findByRole('button', { name: '確定して保存' });
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
  it('何も入っていなければ入力から始め、マインド目標とコミット目標の 2 行を出す', async () => {
    mockApi({});
    render(<DealFirstGoals dealId="901" />);
    fireEvent.click(await screen.findByRole('button', { name: '入力' }));
    expect(await screen.findAllByLabelText('何をするか')).toHaveLength(2);
    expect(screen.getByRole('alert').textContent).toContain('「何をするか」が空の目標があります');
  });
});

it.each(['draft', 'confirmed'] as const)('目標の全体・各目標の注記を%sでも表示し、訂正保存で残す', async status => {
  const note = '数の大小が逆の可能性があります。確かめてください';
  const data: GoalData = { ...draft, status, notes: [note], goals: draft.goals.filter(goal => goal.kind !== 'unclassified').map((goal, i) => i === 0 ? { ...goal, notes: ['数の根拠を確認してください'] } : goal) };
  const patches = mockApi(stored(data));
  render(<DealFirstGoals dealId="901" />);
  expect(await screen.findByText(note)).toBeTruthy();
  expect(screen.getByText('数の根拠を確認してください').tagName).toBe('SMALL');
  expect(screen.getByRole('heading', { name: 'マインド目標 （目指す難易度が高い目標）' })).toBeTruthy();
  expect(screen.getByRole('heading', { name: 'コミット目標 （絶対に達成する目標）' })).toBeTruthy();
  fireEvent.click(await screen.findByRole('button', { name: status === 'draft' ? '確かめて確定' : '訂正' }));
  const first = (await screen.findAllByLabelText('数'))[0];
  if (!first) throw new Error('目標の数の入力がありません');
  fireEvent.change(first, { target: { value: '5名' } });
  fireEvent.click(screen.getByRole('button', { name: '確定して保存' }));
  await waitFor(() => { expect(patches).toHaveLength(1); });
  expect(JSON.parse(patches[0]?.set[GOAL_PROPS.data] ?? '')).toMatchObject({ notes: [note], goals: [{ notes: ['数の根拠を確認してください'] }, {}] });
});

function firstOf(elements: HTMLElement[]): HTMLElement {
  const first = elements[0];
  if (!first) throw new Error('入力がありません');
  return first;
}

describe('初回MTGの目標の両方向の反映', () => {
  const confirmed: GoalData = { ...draft, status: 'confirmed', goals: draft.goals.filter(goal => goal.kind !== 'unclassified') };
  it('案に区分不明が無ければ「この案で確定」で直さずに確定できる', async () => {
    const plain: GoalData = { ...draft, goals: confirmed.goals };
    const values = stored(plain);
    const patches = mockApi({ ...values });
    render(<DealFirstGoals dealId="901" />);
    fireEvent.click(await screen.findByRole('button', { name: 'この案で確定' }));
    await waitFor(() => { expect(patches).toHaveLength(1); });
    expect(patches[0]?.base).toEqual(values);
    expect(patches[0]?.set[GOAL_PROPS.ideal]).toBe(values[GOAL_PROPS.ideal]);
    expect(JSON.parse(patches[0]?.set[GOAL_PROPS.data] ?? '')).toMatchObject({ status: 'confirmed' });
    expect(await screen.findByRole('button', { name: '訂正' })).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'この案で確定' })).toBeNull();
  });
  it('区分不明が残る案には「この案で確定」を出さない', async () => {
    mockApi({ ...stored(draft) });
    render(<DealFirstGoals dealId="901" />);
    expect(await screen.findByRole('button', { name: '確かめて確定' })).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'この案で確定' })).toBeNull();
  });
  it('編集を始めるときに HubSpot の今の値を読み直し、それを base にする', async () => {
    const values: Record<string, string | null> = { ...stored(confirmed) };
    const patches = mockApi(values);
    render(<DealFirstGoals dealId="901" />);
    await screen.findByRole('button', { name: '訂正' });
    const before = patches.reads.length;
    values[GOAL_PROPS.minimum] = '面接を月3件入れる（3件）／契約期間中';
    fireEvent.click(screen.getByRole('button', { name: '訂正' }));
    await screen.findByRole('button', { name: '確定して保存' });
    expect(patches.reads.length).toBe(before + 1);
    expect(patches.reads.at(-1)).toContain('fresh=1');
    expect(screen.getAllByLabelText('何をするか').map(input => (input as HTMLInputElement).value)).toContain('面接を月3件入れる');
    fireEvent.change(firstOf(screen.getAllByLabelText('数')), { target: { value: '6名' } });
    fireEvent.click(screen.getByRole('button', { name: '確定して保存' }));
    await waitFor(() => { expect(patches).toHaveLength(1); });
    expect(patches[0]?.base[GOAL_PROPS.minimum]).toBe('面接を月3件入れる（3件）／契約期間中');
  });
  it('保存の前に HubSpot で変わっていれば上書きせず、今の目標を見せる', async () => {
    const hubspotNow = '面接を月5件入れる（5件）／2027/03まで';
    const patches = mockApi({ ...stored(confirmed) }, true, { [GOAL_PROPS.minimum]: hubspotNow });
    render(<DealFirstGoals dealId="901" />);
    fireEvent.click(await screen.findByRole('button', { name: '訂正' }));
    fireEvent.change(firstOf(await screen.findAllByLabelText('数')), { target: { value: '6名' } });
    fireEvent.click(screen.getByRole('button', { name: '確定して保存' }));
    expect(await screen.findByText('編集を始めたあとに、HubSpot で目標が変わっています。')).toBeTruthy();
    expect(patches).toHaveLength(1);
    expect(screen.getByText(hubspotNow)).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'HubSpot の今の目標を使う' }));
    expect(patches).toHaveLength(1);
  });
  it('HubSpot で直された値を、画面に戻ったとき・一定の間隔で取り込む', async () => {
    const values: Record<string, string | null> = { ...stored(confirmed) };
    const patches = mockApi(values);
    render(<DealFirstGoals dealId="901" refreshMs={50} />);
    await screen.findByRole('button', { name: '訂正' });
    values[GOAL_PROPS.minimum] = '面接を月4件入れる（4件）／契約期間中';
    expect(await screen.findByText('面接を月4件入れる（4件）／契約期間中')).toBeTruthy();
    expect(screen.getByText(/HubSpot で目標の文が直されています/)).toBeTruthy();
    const before = patches.reads.length;
    document.dispatchEvent(new Event('visibilitychange'));
    await waitFor(() => { expect(patches.reads.length).toBeGreaterThan(before); });
    expect(patches.reads.some(url => !url.includes('fresh=1'))).toBe(true);
  });
});
