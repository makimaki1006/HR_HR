// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { DraftSavePanel } from './DraftSavePanel';
import { draftFixture, versionsFixture } from '../job-copy/draftFixtures.test-helper';
import fixtures from '../../generated/jobgen/fixtures.json';
import { initialState, type PipelineState } from './state';
import { createPipelineController } from './pipeline';
import { createStore } from './store';
import type { PostFn } from './api';
afterEach(() => { cleanup(); vi.unstubAllGlobals(); });
it('explicitly selects a job and saves all 84 values and facts, retaining source kind when the input tab changes', async () => {
  const data = versionsFixture(); const bodies: unknown[] = [];
  const fetchMock = vi.fn((url: string, init?: RequestInit) => {
    const value = url.endsWith('/versions') ? data : init?.method === 'POST' ? { status:'saved', draft:draftFixture(), revision:'b'.repeat(64) } : { status:'ready', listings:[data.listing], total:1, titles:[], offset:0, next_offset:null, refreshing:false, refresh_failed:false, index_built_at:'2026-10-11T00:00:00Z' };
    if (init?.method === 'POST') bodies.push(JSON.parse(typeof init.body === 'string' ? init.body : '{}'));
    return Promise.resolve(new Response(JSON.stringify(value),{status:200,headers:{'Content-Type':'application/json'}}));
  }); vi.stubGlobal('fetch',fetchMock);
  const state = { ...initialState(), kind:'pdf' as const, sourceKind:'csv' as const, hrhackerCreatedAt:'2026-10-10T00:00:00Z', sourceText:fixtures.source_text, facts:fixtures.responses.extract.facts, hrhacker:{...fixtures.responses.hrhacker,row:draftFixture().row}, status:{...initialState().status,hrhacker:'review' as const} };
  render(<DraftSavePanel s={state} />); expect(fetchMock).not.toHaveBeenCalled(); fireEvent.click(screen.getByRole('button',{name:'保存先の求人を選ぶ'})); await waitFor(() => { expect(screen.getByRole('button',{name:'架空配送スタッフの版を見る'})).toBeTruthy(); });
  expect(bodies).toHaveLength(0); fireEvent.click(screen.getByRole('button',{name:'架空配送スタッフの版を見る'})); await waitFor(() => { expect(screen.getByRole('button',{name:'この求人に案を保存'})).toBeTruthy(); }); fireEvent.click(screen.getByRole('button',{name:'この求人に案を保存'}));
  await waitFor(() => { expect(screen.getByRole('link',{name:'求人文面管理で今の版と案を比べる'})).toBeTruthy(); });
  expect(bodies).toHaveLength(1); expect(bodies[0]).toMatchObject({base_revision:'a'.repeat(64),source_kind:'csv',created_at:'2026-10-10T00:00:00Z',row:{'基本給与 最小':'270000','基本給与 最大':'300000'},source_text:fixtures.source_text,facts:fixtures.responses.extract.facts}); expect(Object.keys((bodies[0] as {row:Record<string,string>}).row)).toHaveLength(84);
});
it.each<[string, Partial<PipelineState>]>([
  ['元データの読み取り中', { normalizing: true }],
  ['生成中', { running: true }],
  ...(['stale', 'wait', 'run', 'fail'] as const).map(status => [`工程が${status}`, { status: { ...initialState().status, hrhacker: status } }] as [string, Partial<PipelineState>]),
  ['事実が未取得', { facts: null }],
  ['生成日時が未取得', { hrhackerCreatedAt: null }],
  ['案が未取得', { hrhacker: null }],
])('保存先を選んでも%sの案は保存できない', async (_, invalid) => {
  const data = versionsFixture();
  const fetchMock = vi.fn((url: string, init?: RequestInit) => Promise.resolve(new Response(JSON.stringify(url.endsWith('/versions') ? data : init?.method === 'POST' ? { status: 'saved', draft: draftFixture(), revision: 'b'.repeat(64) } : { status: 'ready', listings: [data.listing], total: 1, titles: [], offset: 0, next_offset: null, refreshing: false, refresh_failed: false, index_built_at: '2026-10-11T00:00:00Z' }), { status: 200, headers: { 'Content-Type': 'application/json' } })));
  vi.stubGlobal('fetch', fetchMock);
  const state: PipelineState = { ...initialState(), sourceText: fixtures.source_text, sourceKind: 'csv', hrhackerCreatedAt: '2026-10-10T00:00:00Z', facts: fixtures.responses.extract.facts, hrhacker: fixtures.responses.hrhacker, status: { ...initialState().status, hrhacker: 'done' } };
  const { rerender } = render(<DraftSavePanel s={state} />);
  fireEvent.click(screen.getByRole('button', { name: '保存先の求人を選ぶ' }));
  await waitFor(() => { expect(screen.getByRole('button', { name: '架空配送スタッフの版を見る' })).toBeTruthy(); });
  fireEvent.click(screen.getByRole('button', { name: '架空配送スタッフの版を見る' }));
  await waitFor(() => { expect(screen.getByRole('button', { name: 'この求人に案を保存' }).hasAttribute('disabled')).toBe(false); });
  rerender(<DraftSavePanel s={{ ...state, ...invalid }} />);
  const save = screen.getByRole('button', { name: 'この求人に案を保存' });
  expect(save.hasAttribute('disabled')).toBe(true);
  fireEvent.click(save);
  expect(fetchMock.mock.calls.filter(([, init]) => init?.method === 'POST')).toHaveLength(0);
  rerender(<DraftSavePanel s={state} />);
  expect(screen.getByRole('button', { name: 'この求人に案を保存' }).hasAttribute('disabled')).toBe(false);
  fireEvent.click(screen.getByRole('button', { name: 'この求人に案を保存' }));
  await waitFor(() => { expect(screen.getByRole('link', { name: '求人文面管理で今の版と案を比べる' })).toBeTruthy(); });
  expect(fetchMock.mock.calls.filter(([, init]) => init?.method === 'POST')).toHaveLength(1);
});
it('reading a new multi-job file clears the earlier row and preserves its new input kind', async () => {
  const store = createStore<PipelineState>({...initialState(),sourceText:'営業',hrhacker:fixtures.responses.hrhacker,facts:fixtures.responses.extract.facts});
  const post = vi.fn().mockResolvedValue({ok:true,data:{jobs:[{source_text:'配送',title_hint:'配送'},{source_text:'事務',title_hint:'事務'}]}}) as PostFn;
  const ctl = createPipelineController({store,post}); await ctl.normalize({kind:'csv',text:'職種名\n配送\n事務'}); expect(store.get().hrhacker).toBeNull(); expect(store.get().sourceText).toBe(''); expect(store.get().sourceKind).toBe('csv'); ctl.pickJob(1); expect(store.get().sourceText).toBe('事務');
});
